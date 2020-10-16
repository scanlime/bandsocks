use crate::{
    abi::SyscallInfo,
    process::task::TaskData,
    protocol::{SysPid, VPid},
};
use sc::nr;

#[derive(Debug)]
pub struct SyscallEmulator<'a> {
    task: &'a TaskData,
    info: &'a SyscallInfo,
}

impl<'a> SyscallEmulator<'a> {
    pub fn new(task: &'a TaskData, info: &'a SyscallInfo) -> Self {
        SyscallEmulator { task, info }
    }

    pub async fn dispatch(&mut self) -> isize {
        match self.info.nr as usize {
            nr::ACCESS => self.sys_access().await,
            nr::OPENAT => self.sys_openat().await,
            nr::UNAME => self.sys_uname().await,
            nr::STAT => self.sys_stat().await,
            nr::MMAP => self.sys_mmap().await,
            nr::FSTAT => self.sys_fstat().await,
            n => panic!("unexpected syscall trace, SYS_{} {:?}", n, self),
        }
    }

    async fn sys_access(&mut self) -> isize {
        println!("ACCESS IS NOT HAPPENING {:x?}", self);
        0
    }

    async fn sys_openat(&mut self) -> isize {
        println!("OPENAT NOPE");
        0
    }

    async fn sys_uname(&mut self) -> isize {
        println!("FAKE UNAME A COMIN");
        0
    }

    async fn sys_stat(&mut self) -> isize {
        println!("GET ME A FILESYSTEM STAT");
        0
    }

    async fn sys_fstat(&mut self) -> isize {
        println!("GOT NO TIME TO FSTAT");
        0
    }

    async fn sys_mmap(&mut self) -> isize {
        println!("ARE WE REALLY EMULATING MMAP THO");
        0
    }
}
