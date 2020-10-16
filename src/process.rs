use crate::{
    errors::IPCError,
    sand::protocol::{SysPid, VPtr, VString},
};
use regex::Regex;
use std::{
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    io::Read,
    os::unix::{ffi::OsStrExt, fs::FileExt},
    process::Child,
};

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
pub struct Process {
    sys_pid: SysPid,
    mem_file: File,
}

impl Process {
    pub fn open(sys_pid: SysPid, tracer: &Child) -> Result<Process, IPCError> {
        // Check before and after opening the file, to prevent PID races
        check_can_open(sys_pid, tracer)?;
        let mem_file = open_mem_file(sys_pid)?;
        check_can_open(sys_pid, tracer)?;
        Ok(Process { sys_pid, mem_file })
    }

    pub fn read_bytes(&mut self, vptr: VPtr, buf: &mut [u8]) -> Result<(), IPCError> {
        self.mem_file
            .read_exact_at(buf, vptr.0 as u64)
            .map_err(|_| IPCError::MemAccess)
    }

    pub fn read_string(&mut self, vstr: VString) -> Result<String, IPCError> {
        self.read_string_os(vstr)?
            .into_string()
            .map_err(|_| IPCError::StringDecoding)
    }

    pub fn read_string_os(&mut self, vstr: VString) -> Result<OsString, IPCError> {
        let mut ptr = vstr.0;
        let mut result = OsString::new();
        let mut page_buffer = Vec::with_capacity(*PAGE_SIZE);
        loop {
            page_buffer.resize(page_remaining(ptr), 0u8);
            self.read_bytes(ptr, &mut page_buffer[..]);
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

fn open_mem_file(sys_pid: SysPid) -> Result<File, IPCError> {
    let path = format!("/proc/{}/mem", sys_pid.0);
    Ok(OpenOptions::new().read(true).write(true).open(path)?)
}

fn read_proc_status(sys_pid: SysPid) -> Result<String, IPCError> {
    let path = format!("/proc/{}/status", sys_pid.0);
    let mut file = File::open(path)?;
    let mut string = String::with_capacity(4096);
    file.read_to_string(&mut string);
    Ok(string)
}

fn check_can_open(sys_pid: SysPid, tracer: &Child) -> Result<(), IPCError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"\nPid:\t(\d+)\n.*\nTracerPid:\t(\d+)\n").unwrap();
    }
    let status = read_proc_status(sys_pid)?;
    match RE.captures(&status) {
        None => Err(IPCError::InvalidPid),
        Some(captures) => {
            let pid = captures.get(1).map(|s| s.as_str().parse());
            let tracer_pid = captures.get(2).map(|s| s.as_str().parse());
            if pid == Some(Ok(sys_pid.0)) && tracer_pid == Some(Ok(tracer.id())) {
                Ok(())
            } else {
                Err(IPCError::InvalidPid)
            }
        }
    }
}
