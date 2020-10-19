use crate::{
    abi,
    abi::SyscallInfo,
    process::{loader::Loader, task::StoppedTask},
    protocol::{Errno, FileAccess, FromSand, SysFd, ToSand, VPtr, VString},
};
use sc::nr;

#[derive(Debug)]
pub struct SyscallEmulator<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
    call: SyscallInfo,
}

impl<'q, 's, 't> SyscallEmulator<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let call = SyscallInfo::from_regs(stopped_task.regs);
        SyscallEmulator { stopped_task, call }
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
            nr::EXECVE => {
                let filename = arg_string(0);
                let argv = arg_ptr(1);
                let envp = arg_ptr(2);
                let is_entrypoint_special = self.stopped_task.task.task_data.parent.is_none()
                    && filename == VString(VPtr(0))
                    && argv == VPtr(0)
                    && envp == VPtr(0);
                let loader = if is_entrypoint_special {
                    Loader::from_entrypoint(self.stopped_task)
                } else {
                    Loader::from_execve(self.stopped_task, filename, argv, envp)
                };
                let result = loader.do_exec().await;
                self.return_result(result).await
            }

            nr::ACCESS => {
                let result = ipc_call!(
                    self.stopped_task.task,
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
                    self.stopped_task.task,
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
                    self.stopped_task.task,
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

            _ => panic!("unexpected syscall trace, {:x?}", self),
        };
        SyscallInfo::ret_to_regs(result, self.stopped_task.regs);
    }
}
