use crate::{
    abi,
    abi::SyscallInfo,
    process::task::Task,
    protocol::{Errno, FromSand, SysAccess, SysFd, ToSand, VPtr, VString},
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

    async fn return_sysfd(&mut self, sys_fd: SysFd) -> isize {
        // This will need some kind of trampoline to pass the fd through a socket into
        // the tracee
        println!("unimplemented fd passing, {:?}", sys_fd);
        -abi::EINVAL
    }

    async fn return_errno(&mut self, err: Errno) -> isize {
        assert!(err.0 < 0);
        err.0 as isize
    }

    async fn return_result(&mut self, result: Result<(), Errno>) -> isize {
        match result {
            Ok(()) => 0,
            Err(err) => self.return_errno(err).await,
        }
    }

    async fn return_sysfd_result(&mut self, result: Result<SysFd, Errno>) -> isize {
        match result {
            Ok(sys_fd) => self.return_sysfd(sys_fd).await,
            Err(err) => self.return_errno(err).await,
        }
    }

    pub async fn dispatch(&mut self) -> isize {
        let args = self.call.args;
        match self.call.nr as usize {
            nr::ACCESS => {
                let result = ipc_call!(
                    self.task,
                    FromSand::SysAccess(SysAccess {
                        dir: None,
                        path: VString(VPtr(args[0] as usize)),
                        mode: args[1] as i32,
                    }),
                    ToSand::SysAccessReply(result),
                    result
                );
                self.return_result(result).await
            }

            nr::OPEN => {
                let result = ipc_call!(
                    self.task,
                    FromSand::SysOpen(
                        SysAccess {
                            dir: None,
                            path: VString(VPtr(args[0] as usize)),
                            mode: args[2] as i32,
                        },
                        args[1] as i32
                    ),
                    ToSand::SysOpenReply(result),
                    result
                );
                self.return_sysfd_result(result).await
            }

            nr::OPENAT if args[0] as i32 == abi::AT_FDCWD => {
                let result = ipc_call!(
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
                );
                self.return_sysfd_result(result).await
            }

            _ => panic!("unexpected syscall trace, {:x?}", self),
        }
    }
}
