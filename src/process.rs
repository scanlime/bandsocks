use crate::{
    errors::IPCError,
    sand,
    sand::protocol::{
        deserialize, serialize, Errno, FromSand, MessageFromSand, MessageToSand, SysPid, ToSand,
        BUFFER_SIZE,
    },
};
use memmap::MmapMut;
use regex::Regex;
use std::{
    fs::{File, OpenOptions},
    io::{Cursor, Read},
    os::unix::{io::AsRawFd, prelude::RawFd, process::CommandExt},
    process::Child,
};

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
