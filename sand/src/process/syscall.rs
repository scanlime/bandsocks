use crate::{
    abi::SyscallInfo,
    process::Event,
    process::task::Task,
    protocol::{ToSand, FromSand, Errno},
};
use sc::nr;

#[derive(Debug)]
pub struct SyscallEmulator<'t, 'c, 'q> {
    task: &'t mut Task<'q>,
    call: &'c SyscallInfo,
}

impl<'t, 'c, 'q> SyscallEmulator<'t, 'c, 'q> {
    pub fn new(task: &'t mut Task<'q>, call: &'c SyscallInfo) -> Self {
        SyscallEmulator { task, call }
    }

    pub async fn dispatch(&mut self) -> isize {
        match self.call.nr as usize {
            nr::ACCESS => self.sys_access().await,
            nr::OPENAT => self.sys_openat().await,
            nr::UNAME => self.sys_uname().await,
            nr::STAT => self.sys_stat().await,
            nr::MMAP => self.sys_mmap().await,
            nr::FSTAT => self.sys_fstat().await,
            n => panic!("unexpected syscall trace {:?}", self)
        }
    }

    async fn sys_access(&mut self) -> isize {
        self.task.msg.send(FromSand::SysAccess(self.call.args[0], self.call.args[1]));
        match self.task.events.next().await {
            Event::Message(ToSand::AccessReply(Ok(()))) => 0,
            Event::Message(ToSand::AccessReply(Err(Errno(num)))) => num as isize,
            other => panic!("unexpected sys_access reply {:?}", other)
        }
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
