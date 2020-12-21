use crate::{
    errors::RuntimeError,
    sand::protocol::{ProcessHandle, SysFd, SysPid, VFile, VPtr, VString},
};
use regex::Regex;
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::Read,
    os::unix::{ffi::OsStrExt, fs::FileExt, io::AsRawFd},
};
use tokio::process::Child;

lazy_static! {
    static ref PAGE_SIZE: usize = determine_page_size();
}

fn determine_page_size() -> usize {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
    assert_eq!(page_size & (page_size - 1), 0);
    page_size
}

fn page_offset(vptr: VPtr) -> usize {
    vptr.0 & (*PAGE_SIZE - 1)
}

fn page_remaining(vptr: VPtr) -> usize {
    *PAGE_SIZE - page_offset(vptr)
}

#[derive(Debug)]
pub struct ProcessStatus {
    // todo: uid, gid, loads of other stuff here.
    pub current_dir: VFile,
}

#[derive(Debug)]
pub struct Process {
    sys_pid: SysPid,
    mem_file: File,
    maps_file: File,
    pub status: ProcessStatus,
}

impl Process {
    pub fn open(
        sys_pid: SysPid,
        tracer: &Child,
        status: ProcessStatus,
    ) -> Result<Process, RuntimeError> {
        // Check before and after opening the file, to prevent PID races
        check_can_open(sys_pid, tracer)?;
        let mem_file = open_mem_file(sys_pid)?;
        let maps_file = open_maps_file(sys_pid)?;
        check_can_open(sys_pid, tracer)?;
        Ok(Process {
            sys_pid,
            mem_file,
            maps_file,
            status,
        })
    }

    pub fn to_handle(&self) -> ProcessHandle {
        ProcessHandle {
            mem: SysFd(self.mem_file.as_raw_fd() as u32),
            maps: SysFd(self.maps_file.as_raw_fd() as u32),
        }
    }

    #[allow(dead_code)]
    pub fn read_bytes(&self, vptr: VPtr, buf: &mut [u8]) -> Result<(), RuntimeError> {
        read_bytes(&self.mem_file, vptr, buf)
    }

    pub fn read_string(&self, vstr: VString) -> Result<String, RuntimeError> {
        read_string(&self.mem_file, vstr)
    }

    #[allow(dead_code)]
    pub fn read_string_os(&self, vstr: VString) -> Result<OsString, RuntimeError> {
        read_string_os(&self.mem_file, vstr)
    }
}

fn read_bytes(mem_file: &File, vptr: VPtr, buf: &mut [u8]) -> Result<(), RuntimeError> {
    mem_file
        .read_exact_at(buf, vptr.0 as u64)
        .map_err(|_| RuntimeError::MemAccess)
}

fn read_string(mem_file: &File, vstr: VString) -> Result<String, RuntimeError> {
    read_string_os(mem_file, vstr)?
        .into_string()
        .map_err(|_| RuntimeError::StringDecoding)
}

fn read_string_os(mem_file: &File, vstr: VString) -> Result<OsString, RuntimeError> {
    let mut ptr = vstr.0;
    let mut result = OsString::new();
    let mut page_buffer = Vec::with_capacity(*PAGE_SIZE);
    loop {
        page_buffer.resize(page_remaining(ptr), 0u8);
        read_bytes(mem_file, ptr, &mut page_buffer[..])?;
        match page_buffer.iter().position(|i| *i == 0) {
            None => {
                result.push(OsStr::from_bytes(&page_buffer));
                ptr = ptr.add(page_buffer.len());
            }
            Some(index) => {
                result.push(OsStr::from_bytes(&page_buffer[0..index]));
                break Ok(result);
            }
        }
    }
}

fn open_mem_file(sys_pid: SysPid) -> Result<File, RuntimeError> {
    // open for read only, write is not portable enough
    let path = format!("/proc/{}/mem", sys_pid.0);
    Ok(File::open(path)?)
}

fn open_maps_file(sys_pid: SysPid) -> Result<File, RuntimeError> {
    let path = format!("/proc/{}/maps", sys_pid.0);
    Ok(File::open(path)?)
}

fn read_proc_status(sys_pid: SysPid) -> Result<String, RuntimeError> {
    let path = format!("/proc/{}/status", sys_pid.0);
    let mut file = File::open(path)?;
    let mut string = String::with_capacity(4096);
    file.read_to_string(&mut string)?;
    Ok(string)
}

fn check_can_open(sys_pid: SysPid, tracer: &Child) -> Result<(), RuntimeError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"\nPid:\t(\d+)\n.*\nTracerPid:\t(\d+)\n").unwrap();
    }
    let status = read_proc_status(sys_pid)?;
    match RE.captures(&status) {
        None => Err(RuntimeError::InvalidPid),
        Some(captures) => {
            let pid = captures.get(1).map(|s| s.as_str().parse());
            let tracer_pid = captures.get(2).map(|s| s.as_str().parse());
            if pid == Some(Ok(sys_pid.0)) && tracer_pid == Some(Ok(tracer.id())) {
                Ok(())
            } else {
                Err(RuntimeError::InvalidPid)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_read_from_self() {
        let self_pid = SysPid(unsafe { libc::getpid() as u32 });
        let self_mem = open_mem_file(self_pid).unwrap();

        let page_size = *PAGE_SIZE;
        let map_total_size = 5 * page_size;
        let hole_size = page_size;
        let hole_offset = 3 * page_size;

        let map_addr = unsafe {
            let result = libc::mmap(
                std::ptr::null_mut(),
                map_total_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                -1i32,
                0,
            );
            assert!(result as isize > 0);
            VPtr(result as usize)
        };

        let map_slice =
            unsafe { std::slice::from_raw_parts_mut(map_addr.0 as *mut u8, map_total_size) };

        // We can't allocate a normal guard page to trigger faults, since linux does
        // ptrace reads without checking map permission. Instead, make a hole and rely
        // on it to stay empty. This is racy, and not something that is generally
        // reliable! But uh... unit tests yay.
        unsafe { libc::munmap(map_addr.add(hole_offset).0 as *mut libc::c_void, hole_size) };

        fn is_memaccess_err<T>(result: Result<T, RuntimeError>) -> bool {
            match result {
                Err(RuntimeError::MemAccess) => true,
                _ => false,
            }
        }

        // First test a few edge cases around the memory hole, with all zeroes in the
        // mapping still
        assert_eq!(read_string(&self_mem, VString(map_addr)).unwrap(), "");
        assert_eq!(
            read_string(&self_mem, VString(map_addr.add(hole_offset - 1))).unwrap(),
            ""
        );
        assert!(is_memaccess_err(read_string(
            &self_mem,
            VString(map_addr.add(hole_offset))
        )));
        assert!(is_memaccess_err(read_string(
            &self_mem,
            VString(map_addr.add(hole_offset + hole_size - 1))
        )));
        assert_eq!(
            read_string(&self_mem, VString(map_addr.add(hole_offset + hole_size))).unwrap(),
            ""
        );

        for test_str_size in &[1, 20, 4095, 4096, 4097] {
            for offset in 0..=page_size {
                let mut test_str = String::new();
                while test_str.len() < *test_str_size {
                    test_str.push_str(format!("{}", rand::random::<u64>()).as_str());
                }
                test_str.truncate(*test_str_size);
                let offset_end = offset + test_str.len();
                map_slice[offset..offset_end].copy_from_slice(test_str.as_bytes());
                map_slice[offset_end] = b'\0';
                let readback = read_string(&self_mem, VString(map_addr.add(offset))).unwrap();
                assert_eq!(test_str, readback);
            }
        }

        unsafe { libc::munmap(map_addr.0 as *mut libc::c_void, map_total_size) };
    }
}
