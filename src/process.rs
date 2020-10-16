use crate::{
    errors::IPCError,
    sand::protocol::{SysPid, VPtr, VString},
};
use memmap::{MmapMut, MmapOptions};
use regex::Regex;
use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Read,
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

fn page_floor(vptr: VPtr) -> VPtr {
    VPtr(vptr.0 & !(*PAGE_SIZE - 1))
}

fn page_offset(vptr: VPtr) -> usize {
    vptr.0 & (*PAGE_SIZE - 1)
}

#[derive(Debug)]
pub struct Process {
    sys_pid: SysPid,
    mem_file: File,
    mapped_pages: HashMap<VPtr, MmapMut>,
}

impl Process {
    pub fn open(sys_pid: SysPid, tracer: &Child) -> Result<Process, IPCError> {
        // Check before and after opening the file, to prevent PID races
        check_can_open(sys_pid, tracer)?;
        let mem_file = open_mem_file(sys_pid)?;
        check_can_open(sys_pid, tracer)?;
        Ok(Process {
            sys_pid,
            mem_file,
            mapped_pages: HashMap::new(),
        })
    }

    fn map_page<'a>(&'a mut self, vptr: VPtr) -> Result<&'a MmapMut, IPCError> {
        assert_eq!(page_offset(vptr), 0);
        Err(IPCError::MemAccess)
    }

    pub fn read_string(&mut self, vstr: VString) -> Result<String, IPCError> {
        self.read_str_os(vstr)?
            .into_string()
            .map_err(|_| IPCError::StringDecoding)
    }

    pub fn read_str_os(&mut self, vstr: VString) -> Result<OsString, IPCError> {
        Err(IPCError::MemAccess)
    }

    pub fn read_bytes(&mut self, vptr: VPtr, len: usize) -> Result<Vec<u8>, IPCError> {
        Err(IPCError::MemAccess)
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
