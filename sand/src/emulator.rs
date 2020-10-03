// This code may not be used for any purpose. Be gay, do crime.

use sc::nr;
use crate::abi::SyscallInfo;
use crate::process::{VPid, SysPid};

#[derive(Debug)]
pub struct SyscallEmulator<'a> {
    pid: VPid,
    sys_pid: SysPid,
    info: &'a SyscallInfo,
}

impl<'a> SyscallEmulator<'a> {
    pub fn new(pid: VPid, sys_pid: SysPid, info: &'a SyscallInfo) -> Self {
        SyscallEmulator {
            pid, sys_pid, info
        }
    }

    pub fn dispatch(&mut self) -> isize {
        match self.info.nr as usize {
            nr::ACCESS => self.sys_access(),
            nr::OPENAT => self.sys_openat(),
            nr::UNAME => self.sys_uname(),
            nr::STAT => self.sys_stat(),
            nr::MMAP => self.sys_mmap(),
            nr::FSTAT => self.sys_fstat(),
            n => panic!("unexpected syscall trace, SYS_{} {:?}", n, self)
        }
    }

    fn sys_access(&mut self) -> isize {
        println!("ACCESS IS NOT HAPPENING {:x?}", self);
        0
    }

    fn sys_openat(&mut self) -> isize {
        println!("OPENAT NOPE");
        0
    }

    fn sys_uname(&mut self) -> isize {
        println!("FAKE UNAME A COMIN");
        0
    }

    fn sys_stat(&mut self) -> isize {
        println!("GET ME A FILESYSTEM STAT");
        0
    }

    fn sys_fstat(&mut self) -> isize {
        println!("GOT NO TIME TO FSTAT");
        0
    }

    fn sys_mmap(&mut self) -> isize {
        println!("ARE WE REALLY EMULATING MMAP THO");
        0
    }
}
