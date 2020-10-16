use crate::{
    abi::SyscallInfo,
    process::{task::Task, Event},
    protocol::{Errno, FromSand, ToSand},
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
        let result = match self.call.nr as usize {
            nr::ACCESS => ipc_call!(
                self.task,
                FromSand::SysAccess(self.call.args[0], self.call.args[1]),
                Event::Message(ToSand::AccessReply(result)),
                result
            ),

            _ => panic!("unexpected syscall trace {:?}", self),
        };
        match result {
            Ok(()) => 0,
            Err(Errno(num)) => num as isize,
        }
    }
}
