use crate::{
    abi,
    binformat::Exec,
    mem::string::VStringArray,
    process::task::StoppedTask,
    protocol::{
        abi::Syscall, Errno, FileStat, FollowLinks, FromTask, LogLevel, LogMessage, SysFd, ToTask,
        VFile, VPtr, VString,
    },
    remote::{file::RemoteFd, trampoline::Trampoline},
    syscall,
    syscall::result::SyscallResult,
};
use plain::Plain;
use sc::nr;

#[repr(C)]
struct UserStat(abi::Stat);

#[repr(C)]
struct UserStatFs(abi::StatFs);

unsafe impl Plain for UserStat {}
unsafe impl Plain for UserStatFs {}

#[derive(Debug)]
pub struct SyscallEmulator<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
    call: Syscall,
}

impl<'q, 's, 't> SyscallEmulator<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let call = Syscall::from_regs(stopped_task.regs);
        SyscallEmulator { stopped_task, call }
    }

    async fn return_file(&mut self, vfile: VFile, sys_fd: &SysFd) -> Result<RemoteFd, Errno> {
        let mut tr = Trampoline::new(self.stopped_task);
        let result = syscall::result::file(&mut tr, sys_fd).await;
        if let Ok(fd) = &result {
            self.stopped_task
                .task
                .task_data
                .file_table
                .open(fd.clone(), vfile);
        }
        result
    }

    async fn return_local_bytes(&mut self, bytes: &[u8], to_ptr: VPtr) -> Result<(), Errno> {
        let mut tr = Trampoline::new(self.stopped_task);
        syscall::result::local_bytes(&mut tr, bytes, to_ptr).await
    }

    async fn return_sysfd_bytes(
        &mut self,
        from_fd: &SysFd,
        to_ptr: VPtr,
        len: usize,
    ) -> Result<(), Errno> {
        let mut tr = Trampoline::new(self.stopped_task);
        syscall::result::sysfd_bytes(&mut tr, from_fd, to_ptr, len).await
    }

    async fn return_stat(
        &mut self,
        out_ptr: VPtr,
        vfile: VFile,
        file_stat: &FileStat,
    ) -> Result<(), Errno> {
        let result = UserStat(abi::Stat {
            st_dev: file_stat.st_dev,
            st_ino: vfile.inode as u64,
            st_nlink: file_stat.st_nlink,
            st_mode: file_stat.st_mode,
            st_uid: file_stat.st_uid,
            st_gid: file_stat.st_gid,
            pad0: 0,
            st_rdev: file_stat.st_rdev,
            st_size: file_stat.st_size,
            st_blksize: 4096,
            st_blocks: (file_stat.st_size + 511) / 512,
            st_atime: file_stat.st_atime,
            st_atime_nsec: file_stat.st_atime_nsec,
            st_mtime: file_stat.st_mtime,
            st_mtime_nsec: file_stat.st_mtime_nsec,
            st_ctime: file_stat.st_ctime,
            st_ctime_nsec: file_stat.st_ctime_nsec,
            unused: [0; 3],
        });
        self.return_local_bytes(unsafe { plain::as_bytes(&result) }, out_ptr)
            .await
    }

    async fn return_statfs(&mut self, out_ptr: VPtr) -> Result<(), Errno> {
        let result = UserStatFs(abi::StatFs {
            f_type: 0,
            f_bsize: 0,
            f_blocks: 0,
            f_bfree: 0,
            f_bavail: 0,
            f_files: 0,
            f_ffree: 0,
            f_fsid: [0; 2],
            f_namelen: 0,
            f_frsize: 0,
            f_flags: 0,
            f_spare: [0; 4],
        });
        self.return_local_bytes(unsafe { plain::as_bytes(&result) }, out_ptr)
            .await
    }

    async fn return_file_result(
        &mut self,
        result: Result<(VFile, SysFd), Errno>,
    ) -> Result<RemoteFd, Errno> {
        match result {
            Err(err) => Err(err),
            Ok((vfile, sys_fd)) => self.return_file(vfile, &sys_fd).await,
        }
    }

    async fn return_stat_result(
        &mut self,
        out_ptr: VPtr,
        result: Result<(VFile, FileStat), Errno>,
    ) -> Result<(), Errno> {
        let (vfile, file_stat) = result?;
        self.return_stat(out_ptr, vfile, &file_stat).await
    }

    async fn return_bytes_result(
        &mut self,
        result: Result<(SysFd, usize), Errno>,
        buffer: VPtr,
        buffer_len: usize,
    ) -> Result<usize, Errno> {
        let (result_fd, result_len) = result?;
        let actual_len = buffer_len.min(result_len);
        self.return_sysfd_bytes(&result_fd, buffer, actual_len)
            .await?;
        Ok(actual_len)
    }

    pub async fn dispatch(&mut self) {
        let args = self.call.args;
        let arg_u32 = |idx| args[idx] as u32;
        let arg_i32 = |idx| args[idx] as i32;
        let arg_usize = |idx| args[idx] as usize;
        let arg_ptr = |idx| VPtr(arg_usize(idx));
        let arg_string = |idx| VString(arg_ptr(idx));
        let arg_fd = |idx| RemoteFd(arg_u32(idx));
        let mut log_level = LogLevel::Debug;
        let result: SyscallResult = match self.call.nr as usize {
            nr::BRK => syscall::user::brk(self.stopped_task, arg_ptr(0))
                .await
                .into(),

            nr::EXECVE => Exec {
                filename: arg_string(0),
                argv: VStringArray(arg_ptr(1)),
                envp: VStringArray(arg_ptr(2)),
            }
            .load(self.stopped_task)
            .await
            .into(),

            nr::UNAME => syscall::user::uname(self.stopped_task, arg_ptr(0))
                .await
                .into(),

            nr::DUP => syscall::fs::dup(self.stopped_task, arg_fd(0)).await.into(),

            nr::DUP2 => syscall::fs::dup2(self.stopped_task, arg_fd(0), arg_fd(1))
                .await
                .into(),

            nr::GETPID => self.stopped_task.task.task_data.vpid.into(),
            nr::GETTID => self.stopped_task.task.task_data.vpid.into(),

            nr::GETPPID => SyscallResult(1),
            nr::GETUID => SyscallResult(0),
            nr::GETGID => SyscallResult(0),
            nr::GETEUID => SyscallResult(0),
            nr::GETEGID => SyscallResult(0),
            nr::GETPGRP => SyscallResult(0),
            nr::SETPGID => SyscallResult(0),
            nr::GETPGID => SyscallResult(0),

            nr::SYSINFO => SyscallResult(0),

            nr::SET_TID_ADDRESS => SyscallResult(0),

            nr::WAIT4 => Errno(-abi::ECHILD).into(),

            nr::FORK => panic!("fork"),
            nr::CLONE => panic!("clone"),

            nr::IOCTL => {
                let _fd = arg_fd(0);
                let _cmd = arg_i32(1);
                let _arg = arg_usize(2);
                SyscallResult(0)
            }

            nr::STAT => ipc_call!(
                self.stopped_task.task,
                FromTask::FileStat {
                    file: None,
                    path: Some(arg_string(0)),
                    follow_links: FollowLinks::Follow,
                },
                ToTask::FileStatReply(result),
                self.return_stat_result(arg_ptr(1), result).await.into()
            ),

            nr::FSTAT => {
                let result = syscall::fs::fstat(self.stopped_task, arg_fd(0)).await;
                self.return_stat_result(arg_ptr(1), result).await.into()
            }

            nr::LSTAT => ipc_call!(
                self.stopped_task.task,
                FromTask::FileStat {
                    file: None,
                    path: Some(arg_string(0)),
                    follow_links: FollowLinks::NoFollow,
                },
                ToTask::FileStatReply(result),
                self.return_stat_result(arg_ptr(1), result).await.into()
            ),

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
                        file: None,
                        path: Some(arg_string(1)),
                        follow_links: if (flags & abi::AT_SYMLINK_NOFOLLOW) != 0 {
                            FollowLinks::NoFollow
                        } else {
                            FollowLinks::Follow
                        }
                    },
                    ToTask::FileStatReply(result),
                    result
                );
                self.return_stat_result(arg_ptr(2), result).await.into()
            }

            nr::STATFS => self.return_statfs(arg_ptr(1)).await.into(),
            nr::FSTATFS => self.return_statfs(arg_ptr(1)).await.into(),

            nr::ACCESS => ipc_call!(
                self.stopped_task.task,
                FromTask::FileAccess {
                    dir: None,
                    path: arg_string(0),
                    mode: arg_i32(1),
                },
                ToTask::Reply(result),
                result.into()
            ),

            nr::GETCWD => ipc_call!(
                self.stopped_task.task,
                FromTask::GetWorkingDir,
                ToTask::BytesReply(result),
                self.return_bytes_result(result, arg_ptr(0), arg_usize(1))
                    .await
                    .into()
            ),

            nr::READLINK => ipc_call!(
                self.stopped_task.task,
                FromTask::ReadLink(arg_string(0)),
                ToTask::BytesReply(result),
                self.return_bytes_result(result, arg_ptr(1), arg_usize(2))
                    .await
                    .into()
            ),

            nr::GETDENTS64 => {
                syscall::fs::getdents(self.stopped_task, arg_fd(0), arg_ptr(1), arg_usize(2))
                    .await
            }

            nr::CHDIR => ipc_call!(
                self.stopped_task.task,
                FromTask::ChangeWorkingDir(arg_string(0)),
                ToTask::Reply(result),
                result.into()
            ),

            nr::FCHDIR => SyscallResult(0),

            nr::OPEN => ipc_call!(
                self.stopped_task.task,
                FromTask::FileOpen {
                    dir: None,
                    path: arg_string(0),
                    flags: arg_i32(1),
                    mode: arg_i32(2),
                },
                ToTask::FileReply(result),
                self.return_file_result(result).await.into()
            ),

            nr::CLOSE => syscall::fs::close(self.stopped_task, arg_fd(0))
                .await
                .into(),

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
                self.return_file_result(result).await.into()
            }

            _ => panic!("unexpected {:?}", self.call),
        };
        self.call.ret = result.0;
        Syscall::ret_to_regs(self.call.ret, self.stopped_task.regs);

        if self.stopped_task.task.log_enabled(log_level) {
            self.stopped_task
                .task
                .log(log_level, LogMessage::Emulated(self.call.clone()))
        }
    }
}
