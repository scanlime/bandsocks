use crate::{
    errors::RuntimeError,
    filesystem::vfs::VFile,
    sand::protocol::{ProcessHandle, SysFd, SysPid, VPtr, VString},
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

    pub fn read_bytes(&self, vptr: VPtr, buf: &mut [u8]) -> Result<(), RuntimeError> {
        self.mem_file
            .read_exact_at(buf, vptr.0 as u64)
            .map_err(|_| RuntimeError::MemAccess)
    }

    pub fn read_string(&self, vstr: VString) -> Result<String, RuntimeError> {
        self.read_string_os(vstr)?
            .into_string()
            .map_err(|_| RuntimeError::StringDecoding)
    }

    pub fn read_string_os(&self, vstr: VString) -> Result<OsString, RuntimeError> {
        let mut ptr = vstr.0;
        let mut result = OsString::new();
        let mut page_buffer = Vec::with_capacity(*PAGE_SIZE);
        loop {
            page_buffer.resize(page_remaining(ptr), 0u8);
            self.read_bytes(ptr, &mut page_buffer[..])?;
            match page_buffer.iter().position(|i| *i == 0) {
                None => {
                    result.push(OsStr::from_bytes(&page_buffer));
                    ptr = VPtr(ptr.0 + page_buffer.len());
                }
                Some(index) => {
                    result.push(OsStr::from_bytes(&page_buffer[0..index]));
                    break Ok(result);
                }
            }
            result.push(OsStr::from_bytes(&page_buffer));
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
