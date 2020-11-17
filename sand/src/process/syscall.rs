use crate::{
    abi,
    abi::SyscallInfo,
    process::{loader::Loader, task::StoppedTask},
    protocol::{Errno, FromTask, SysFd, ToTask, VPtr, VString},
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
        // to do, use remote syscall trampoline to pass fd into process
        println!("unimplemented fd passing, {:?}", sys_fd);
        -abi::EINVAL as isize
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

    async fn return_file_result(&mut self, result: Result<SysFd, Errno>) -> isize {
        match result {
            Ok(f) => self.return_sysfd(f).await,
            Err(err) => self.return_errno(err).await,
        }
    }

    pub async fn dispatch(&mut self) {
        let args = self.call.args;
        let arg_i32 = |idx| args[idx] as i32;
        let arg_ptr = |idx| VPtr(args[idx] as usize);
        let arg_string = |idx| VString(arg_ptr(idx));

        let result = match self.call.nr as usize {
            nr::BRK => {
                let arg_brk = arg_ptr(0);
                let mm = &mut self.stopped_task.task.task_data.mm;
                let old_brk = mm.brk;
                println!("brk {:x?} -> {:x?}", old_brk, arg_brk);
                old_brk.0 as isize
            }

            nr::EXECVE => {
                let filename = arg_string(0);
                let argv = arg_ptr(1);
                let envp = arg_ptr(2);
                let result = Loader::execve(self.stopped_task, filename, argv, envp).await;
                self.return_result(result).await
            }

            nr::ACCESS => {
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::FileAccess {
                        dir: None,
                        path: arg_string(0),
                        mode: arg_i32(1),
                    },
                    ToTask::Reply(result),
                    result
                );
                self.return_result(result).await
            }

            nr::CHDIR => {
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::ChDir(arg_string(0)),
                    ToTask::Reply(result),
                    result
                );
                self.return_result(result).await
            }

            nr::OPEN => {
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::FileOpen {
                        dir: None,
                        path: arg_string(0),
                        flags: arg_i32(1),
                        mode: arg_i32(2),
                    },
                    ToTask::FileReply(result),
                    result
                );
                self.return_file_result(result).await
            }

            nr::OPENAT if arg_i32(0) == abi::AT_FDCWD => {
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::FileOpen {
                        dir: None,
                        path: arg_string(1),
                        flags: arg_i32(2),
                        mode: arg_i32(3),
                    },
                    ToTask::FileReply(result),
                    result
                );
                self.return_file_result(result).await
            }

            _ => panic!("unexpected syscall trace, {:x?}", self),
        };
        SyscallInfo::ret_to_regs(result, self.stopped_task.regs);
    }
}
