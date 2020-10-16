use crate::{
    abi,
    abi::SyscallInfo,
    process::task::Task,
    protocol::{Errno, FromSand, SysAccess, ToSand, VPtr, VString},
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
        let args = self.call.args;
        match self.call.nr as usize {
            nr::ACCESS => match ipc_call!(
                self.task,
                FromSand::SysAccess(SysAccess {
                    dir: None,
                    path: VString(VPtr(args[0] as usize)),
                    mode: args[1] as i32,
                }),
                ToSand::SysAccessReply(result),
                result
            ) {
                Ok(()) => 0,
                Err(Errno(num)) => num as isize,
            },

            nr::OPENAT if args[0] as i32 == abi::AT_FDCWD => match ipc_call!(
                self.task,
                FromSand::SysOpen(
                    SysAccess {
                        dir: None,
                        path: VString(VPtr(args[1] as usize)),
                        mode: args[3] as i32,
                    },
                    args[2] as i32
                ),
                ToSand::SysOpenReply(result),
                result
            ) {
                Ok(_file) => -abi::EINVAL,
                Err(Errno(num)) => num as isize,
            },

            _ => panic!("unexpected syscall trace, {:x?}", self),
        }
    }
}
