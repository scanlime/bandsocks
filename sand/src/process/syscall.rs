use crate::{
    abi,
    abi::{SyscallInfo, UserRegs},
    process::{remote, task::Task},
    protocol::{Errno, FileAccess, FromSand, SysFd, ToSand, VPtr, VString},
};
use sc::nr;

#[derive(Debug)]
pub struct SyscallEmulator<'t, 'c, 'q> {
    task: &'t mut Task<'q>,
    regs: &'c mut UserRegs,
    call: &'c SyscallInfo,
}

impl<'t, 'c, 'q> SyscallEmulator<'t, 'c, 'q> {
    pub fn new(task: &'t mut Task<'q>, regs: &'c mut UserRegs, call: &'c SyscallInfo) -> Self {
        SyscallEmulator { task, regs, call }
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

    pub async fn dispatch(&mut self) {
        let args = self.call.args;
        let arg_i32 = |idx| args[idx] as i32;
        let arg_ptr = |idx| VPtr(args[idx] as usize);
        let arg_string = |idx| VString(arg_ptr(idx));

        let result = match self.call.nr as usize {
            nr::ACCESS => {
                let result = ipc_call!(
                    self.task,
                    FromSand::FileAccess(FileAccess {
                        dir: None,
                        path: arg_string(0),
                        mode: arg_i32(1),
                    }),
                    ToSand::FileAccessReply(result),
                    result
                );
                self.return_result(result).await
            }

            nr::OPEN => {
                let result = ipc_call!(
                    self.task,
                    FromSand::FileOpen {
                        file: FileAccess {
                            dir: None,
                            path: arg_string(0),
                            mode: arg_i32(2),
                        },
                        flags: arg_i32(1),
                    },
                    ToSand::FileOpenReply(result),
                    result
                );
                self.return_sysfd_result(result).await
            }

            nr::OPENAT if arg_i32(0) == abi::AT_FDCWD => {
                let result = ipc_call!(
                    self.task,
                    FromSand::FileOpen {
                        file: FileAccess {
                            dir: None,
                            path: arg_string(1),
                            mode: arg_i32(3),
                        },
                        flags: arg_i32(2)
                    },
                    ToSand::FileOpenReply(result),
                    result
                );
                self.return_sysfd_result(result).await
            }

            nr::EXECVE => {
                println!("made it to exec! with {:?}", self.task);

                println!(
                    "remote pid, {}",
                    remote::syscall(self.task, nr::GETPID, &[]).await
                );

                self.return_result(Err(Errno(-1))).await
            }

            _ => panic!("unexpected syscall trace, {:x?}", self),
        };
        SyscallInfo::ret_to_regs(result, &mut self.regs);
    }
}
