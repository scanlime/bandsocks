use crate::{
    abi,
    abi::SyscallInfo,
    process::{loader::Loader, task::StoppedTask},
    protocol::{
        Errno, FileStat, FromTask, LogLevel, LogMessage, LogSyscall, SysFd, ToTask, VPtr, VString,
    },
    remote::{scratchpad::Scratchpad, trampoline::Trampoline, RemoteFd},
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
        let mut tr = Trampoline::new(self.stopped_task);
        let result = match Scratchpad::new(&mut tr).await {
            Err(err) => Err(err),
            Ok(mut scratchpad) => {
                let result = scratchpad.send_fd(&sys_fd).await;
                scratchpad.free().await.expect("leaking scratchpad page");
                result
            }
        };
        match result {
            Ok(RemoteFd(fd)) => fd as isize,
            Err(err) => self.return_errno(err).await,
        }
    }

    async fn return_errno(&mut self, err: Errno) -> isize {
        if err.0 >= 0 {
            panic!("invalid {:?}", err);
        }
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

    async fn return_stat_result(
        &mut self,
        _out_ptr: VPtr,
        _result: Result<FileStat, Errno>,
    ) -> isize {
        // to do
        -1
    }

    async fn return_vptr_result(&mut self, result: Result<VPtr, Errno>) -> isize {
        match result {
            Ok(ptr) => ptr.0 as isize,
            Err(err) => self.return_errno(err).await,
        }
    }

    async fn return_size_result(&mut self, result: Result<usize, Errno>) -> isize {
        match result {
            Ok(s) => s as isize,
            Err(err) => self.return_errno(err).await,
        }
    }

    pub async fn dispatch(&mut self) {
        let args = self.call.args;
        let arg_i32 = |idx| args[idx] as i32;
        let arg_usize = |idx| args[idx] as usize;
        let arg_ptr = |idx| VPtr(arg_usize(idx));
        let arg_string = |idx| VString(arg_ptr(idx));
        let mut log_level = LogLevel::Debug;

        let result = match self.call.nr as usize {
            nr::BRK => {
                let ptr = arg_ptr(0);
                let result = do_brk(self.stopped_task, ptr).await;
                self.return_vptr_result(result).await
            }

            nr::EXECVE => {
                let filename = arg_string(0);
                let argv = arg_ptr(1);
                let envp = arg_ptr(2);
                let result = Loader::execve(self.stopped_task, filename, argv, envp).await;
                self.return_result(result).await
            }

            nr::GETPID => self.stopped_task.task.task_data.vpid.0 as isize,

            nr::GETPPID => {
                log_level = LogLevel::Warn;
                1
            }

            nr::GETUID => {
                log_level = LogLevel::Warn;
                0
            }

            nr::GETGID => {
                log_level = LogLevel::Warn;
                0
            }

            nr::GETEUID => {
                log_level = LogLevel::Warn;
                0
            }

            nr::GETEGID => {
                log_level = LogLevel::Warn;
                0
            }

            nr::GETPGRP => {
                log_level = LogLevel::Warn;
                0
            }

            nr::SETPGID => {
                log_level = LogLevel::Warn;
                0
            }

            nr::GETPGID => {
                log_level = LogLevel::Warn;
                0
            }

            nr::UNAME => {
                log_level = LogLevel::Warn;
                0
            }

            nr::SYSINFO => {
                log_level = LogLevel::Warn;
                0
            }

            nr::SET_TID_ADDRESS => {
                log_level = LogLevel::Warn;
                0
            }

            nr::IOCTL => {
                log_level = LogLevel::Warn;
                let _fd = arg_i32(0);
                let _cmd = arg_i32(1);
                let _arg = arg_usize(2);
                0
            }

            nr::STAT => {
                log_level = LogLevel::Warn;
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::FileStat {
                        fd: None,
                        path: Some(arg_string(0)),
                        nofollow: false
                    },
                    ToTask::FileStatReply(result),
                    result
                );
                self.return_stat_result(arg_ptr(1), result).await
            }

            nr::FSTAT => {
                log_level = LogLevel::Warn;
                0
            }

            nr::LSTAT => {
                log_level = LogLevel::Warn;
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::FileStat {
                        fd: None,
                        path: Some(arg_string(0)),
                        nofollow: true
                    },
                    ToTask::FileStatReply(result),
                    result
                );
                self.return_stat_result(arg_ptr(1), result).await
            }

            nr::NEWFSTATAT => {
                log_level = LogLevel::Warn;
                let flags = arg_i32(3);
                let fd = arg_i32(0);
                if fd != abi::AT_FDCWD {
                    unimplemented!();
                }
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::FileStat {
                        fd: None,
                        path: Some(arg_string(1)),
                        nofollow: (flags & abi::AT_SYMLINK_NOFOLLOW) != 0
                    },
                    ToTask::FileStatReply(result),
                    result
                );
                self.return_stat_result(arg_ptr(2), result).await
            }

            nr::ACCESS => {
                log_level = LogLevel::Warn;
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

            nr::GETCWD => {
                log_level = LogLevel::Warn;
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::GetWorkingDir(arg_string(0), arg_usize(1)),
                    ToTask::SizeReply(result),
                    result
                );
                self.return_size_result(result).await
            }

            nr::CHDIR => {
                log_level = LogLevel::Warn;
                let result = ipc_call!(
                    self.stopped_task.task,
                    FromTask::ChangeWorkingDir(arg_string(0)),
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
                let fd = arg_i32(0);
                if fd != abi::AT_FDCWD {
                    log_level = LogLevel::Error;
                }
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

            _ => {
                log_level = LogLevel::Error;
                self.return_result(Err(Errno(-abi::ENOSYS))).await
            }
        };
        SyscallInfo::ret_to_regs(result, self.stopped_task.regs);

        if self.stopped_task.task.log_enabled(log_level) {
            self.stopped_task.task.log(
                log_level,
                LogMessage::Emulated(
                    LogSyscall(self.call.nr, self.call.args, result),
                    VPtr(self.call.instruction_pointer as usize),
                ),
            )
        }
    }
}

async fn do_brk<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    ptr: VPtr,
) -> Result<VPtr, Errno> {
    let mm = stopped_task.task.task_data.mm.clone();
    assert_eq!(0, abi::page_offset(mm.brk.0));
    assert_eq!(0, abi::page_offset(mm.brk_start.0));
    let new_brk = VPtr(abi::page_round_up(ptr.max(mm.brk_start).0));
    if new_brk != mm.brk {
        let mut tr = Trampoline::new(stopped_task);
        if new_brk == mm.brk_start {
            tr.munmap(mm.brk_start, mm.brk.0 - mm.brk_start.0).await?;
        } else if mm.brk == mm.brk_start {
            tr.mmap_anonymous(
                mm.brk_start,
                new_brk.0 - mm.brk_start.0,
                abi::PROT_READ | abi::PROT_WRITE,
            )
            .await?;
        } else {
            tr.mremap(
                mm.brk_start,
                mm.brk.0 - mm.brk_start.0,
                new_brk.0 - mm.brk_start.0,
            )
            .await?;
        }
        tr.stopped_task.task.task_data.mm.brk = new_brk;
    }
    Ok(new_brk)
}
