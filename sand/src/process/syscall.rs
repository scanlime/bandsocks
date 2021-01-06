use crate::{
    abi,
    binformat::Exec,
    mem::{
        maps::{MappedPages, MemFlags},
        page::VPage,
        string::VStringArray,
    },
    process::task::StoppedTask,
    protocol::{
        abi::Syscall, Errno, FileStat, FollowLinks, FromTask, LogLevel, LogMessage, SysFd, ToTask,
        VFile, VPtr, VString, VStringBuffer,
    },
    remote::{
        file::{RemoteFd, TempRemoteFd},
        scratchpad::Scratchpad,
        trampoline::Trampoline,
    },
};
use plain::Plain;
use sc::nr;

#[derive(Debug)]
pub struct SyscallEmulator<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
    call: Syscall,
}

#[repr(C)]
struct UserStat(abi::Stat);

#[repr(C)]
struct UserStatFs(abi::StatFs);

unsafe impl Plain for UserStat {}
unsafe impl Plain for UserStatFs {}

fn return_errno(err: Errno) -> isize {
    if err.0 >= 0 {
        panic!("invalid {:?}", err);
    }
    err.0 as isize
}

fn return_result(result: Result<(), Errno>) -> isize {
    match result {
        Ok(()) => 0,
        Err(err) => return_errno(err),
    }
}

fn return_vptr_result(result: Result<VPtr, Errno>) -> isize {
    match result {
        Ok(ptr) => ptr.0 as isize,
        Err(err) => return_errno(err),
    }
}

fn return_size_result(result: Result<usize, Errno>) -> isize {
    match result {
        Ok(s) => s as isize,
        Err(err) => return_errno(err),
    }
}

impl<'q, 's, 't> SyscallEmulator<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let call = Syscall::from_regs(stopped_task.regs);
        SyscallEmulator { stopped_task, call }
    }

    async fn return_file(&mut self, vfile: VFile, sys_fd: SysFd) -> isize {
        let mut tr = Trampoline::new(self.stopped_task);
        let result = match Scratchpad::new(&mut tr).await {
            Err(err) => Err(err),
            Ok(mut pad) => {
                let main_result = RemoteFd::from_local(&mut pad, &sys_fd).await;
                let cleanup_result = pad.free().await;
                match (main_result, cleanup_result) {
                    (Ok(r), Ok(())) => Ok(r),
                    (Err(e), _) => Err(e),
                    (Ok(_), Err(e)) => Err(e),
                }
            }
        };
        match result {
            Ok(RemoteFd(fd)) => {
                self.stopped_task
                    .task
                    .task_data
                    .file_table
                    .open(RemoteFd(fd), vfile);
                fd as isize
            }
            Err(err) => return_errno(err),
        }
    }

    async fn return_stat(&mut self, out_ptr: VPtr, vfile: VFile, file_stat: FileStat) -> isize {
        let user_stat = UserStat(abi::Stat {
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
        let mut tr = Trampoline::new(self.stopped_task);
        let result = Scratchpad::new(&mut tr).await;
        let result = match result {
            Err(err) => Err(err),
            Ok(mut pad) => {
                let main_result = match TempRemoteFd::new(&mut pad).await {
                    Err(err) => Err(err),
                    Ok(temp) => {
                        let main_result = temp
                            .mem_write_bytes_exact(&mut pad, out_ptr, unsafe {
                                plain::as_bytes(&user_stat)
                            })
                            .await;
                        let cleanup_result = temp.free(&mut pad.trampoline).await;
                        match (main_result, cleanup_result) {
                            (Ok(r), Ok(())) => Ok(r),
                            (Err(e), _) => Err(e),
                            (Ok(_), Err(e)) => Err(e),
                        }
                    }
                };
                let cleanup_result = pad.free().await;
                match (main_result, cleanup_result) {
                    (Ok(r), Ok(())) => Ok(r),
                    (Err(e), _) => Err(e),
                    (Ok(_), Err(e)) => Err(e),
                }
            }
        };
        return_result(result)
    }

    async fn return_statfs(&mut self, out_ptr: VPtr) -> isize {
        let user_statfs = UserStatFs(abi::StatFs {
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
        let mut tr = Trampoline::new(self.stopped_task);
        let result = Scratchpad::new(&mut tr).await;
        let result = match result {
            Err(err) => Err(err),
            Ok(mut pad) => {
                let main_result = match TempRemoteFd::new(&mut pad).await {
                    Err(err) => Err(err),
                    Ok(temp) => {
                        let main_result = temp
                            .mem_write_bytes_exact(&mut pad, out_ptr, unsafe {
                                plain::as_bytes(&user_statfs)
                            })
                            .await;
                        let cleanup_result = temp.free(&mut pad.trampoline).await;
                        match (main_result, cleanup_result) {
                            (Ok(r), Ok(())) => Ok(r),
                            (Err(e), _) => Err(e),
                            (Ok(_), Err(e)) => Err(e),
                        }
                    }
                };
                let cleanup_result = pad.free().await;
                match (main_result, cleanup_result) {
                    (Ok(r), Ok(())) => Ok(r),
                    (Err(e), _) => Err(e),
                    (Ok(_), Err(e)) => Err(e),
                }
            }
        };
        return_result(result)
    }

    async fn return_file_result(&mut self, result: Result<(VFile, SysFd), Errno>) -> isize {
        match result {
            Ok((vfile, sys_fd)) => self.return_file(vfile, sys_fd).await,
            Err(err) => return_errno(err),
        }
    }

    async fn return_stat_result(
        &mut self,
        out_ptr: VPtr,
        result: Result<(VFile, FileStat), Errno>,
    ) -> isize {
        match result {
            Ok((vfile, file_stat)) => self.return_stat(out_ptr, vfile, file_stat).await,
            Err(err) => return_errno(err),
        }
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
        let result = match self.call.nr as usize {
            nr::BRK => return_vptr_result(do_brk(self.stopped_task, arg_ptr(0)).await),

            nr::EXECVE => return_result(
                Exec {
                    filename: arg_string(0),
                    argv: VStringArray(arg_ptr(1)),
                    envp: VStringArray(arg_ptr(2)),
                }
                .load(self.stopped_task)
                .await,
            ),

            nr::UNAME => return_result(do_uname(self.stopped_task, arg_ptr(0)).await),

            nr::GETPID => self.stopped_task.task.task_data.vpid.0 as isize,
            nr::GETTID => self.stopped_task.task.task_data.vpid.0 as isize,

            nr::GETPPID => 1,
            nr::GETUID => 0,
            nr::GETGID => 0,
            nr::GETEUID => 0,
            nr::GETEGID => 0,
            nr::GETPGRP => 0,
            nr::SETPGID => 0,
            nr::GETPGID => 0,

            nr::SYSINFO => 0,

            nr::SET_TID_ADDRESS => 0,

            nr::WAIT4 => return_result(Err(Errno(-abi::ECHILD))),

            nr::FORK => panic!("fork"),
            nr::CLONE => panic!("clone"),

            nr::IOCTL => {
                let _fd = arg_fd(0);
                let _cmd = arg_i32(1);
                let _arg = arg_usize(2);
                0
            }

            nr::GETDENTS64 => {
                let _fd = arg_fd(0);
                let _dirent = arg_ptr(1);
                let _count = arg_u32(2);
                0
            }

            nr::STAT => ipc_call!(
                self.stopped_task.task,
                FromTask::FileStat {
                    file: None,
                    path: Some(arg_string(0)),
                    follow_links: FollowLinks::Follow,
                },
                ToTask::FileStatReply(result),
                self.return_stat_result(arg_ptr(1), result).await
            ),

            nr::FSTAT => {
                let result = do_fstat(self.stopped_task, arg_fd(0)).await;
                self.return_stat_result(arg_ptr(1), result).await
            }

            nr::LSTAT => ipc_call!(
                self.stopped_task.task,
                FromTask::FileStat {
                    file: None,
                    path: Some(arg_string(0)),
                    follow_links: FollowLinks::NoFollow,
                },
                ToTask::FileStatReply(result),
                self.return_stat_result(arg_ptr(1), result).await
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
                self.return_stat_result(arg_ptr(2), result).await
            }

            nr::STATFS => self.return_statfs(arg_ptr(1)).await,
            nr::FSTATFS => self.return_statfs(arg_ptr(1)).await,

            nr::ACCESS => ipc_call!(
                self.stopped_task.task,
                FromTask::FileAccess {
                    dir: None,
                    path: arg_string(0),
                    mode: arg_i32(1),
                },
                ToTask::Reply(result),
                return_result(result)
            ),

            nr::GETCWD => ipc_call!(
                self.stopped_task.task,
                FromTask::GetWorkingDir(VStringBuffer(arg_string(0), arg_usize(1))),
                ToTask::SizeReply(result),
                return_size_result(result)
            ),

            nr::READLINK => ipc_call!(
                self.stopped_task.task,
                FromTask::ReadLink(arg_string(0), VStringBuffer(arg_string(1), arg_usize(2))),
                ToTask::SizeReply(result),
                return_size_result(result)
            ),

            nr::CHDIR => ipc_call!(
                self.stopped_task.task,
                FromTask::ChangeWorkingDir(arg_string(0)),
                ToTask::Reply(result),
                return_result(result)
            ),

            nr::FCHDIR => 0,

            nr::OPEN => ipc_call!(
                self.stopped_task.task,
                FromTask::FileOpen {
                    dir: None,
                    path: arg_string(0),
                    flags: arg_i32(1),
                    mode: arg_i32(2),
                },
                ToTask::FileReply(result),
                self.return_file_result(result).await
            ),

            nr::CLOSE => return_result(do_close(self.stopped_task, arg_fd(0)).await),

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

            _ => panic!("unexpected {:?}", self.call),
        };
        self.call.ret = result;
        Syscall::ret_to_regs(self.call.ret, self.stopped_task.regs);

        if self.stopped_task.task.log_enabled(log_level) {
            self.stopped_task
                .task
                .log(log_level, LogMessage::Emulated(self.call.clone()))
        }
    }
}

async fn do_fstat<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    fd: RemoteFd,
) -> Result<(VFile, FileStat), Errno> {
    let file = stopped_task.task.task_data.file_table.get(&fd)?;
    ipc_call!(
        stopped_task.task,
        FromTask::FileStat {
            file: Some(file.clone()),
            path: None,
            follow_links: FollowLinks::Follow,
        },
        ToTask::FileStatReply(result),
        result
    )
}

async fn do_close<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    fd: RemoteFd,
) -> Result<(), Errno> {
    // Note that the fd will be closed even if close() also reports an error
    stopped_task.task.task_data.file_table.close(&fd);
    let mut tr = Trampoline::new(stopped_task);
    fd.close(&mut tr).await
}

async fn do_uname<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    dest: VPtr,
) -> Result<(), Errno> {
    let mut tr = Trampoline::new(stopped_task);
    let mut pad = Scratchpad::new(&mut tr).await?;
    let main_result = match TempRemoteFd::new(&mut pad).await {
        Err(err) => Err(err),
        Ok(temp) => {
            let main_result = Ok(());
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, sysname),
                    b"Linux\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, nodename),
                    b"host\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, release),
                    b"4.0.0-bandsocks\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, version),
                    b"#1 SMP\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, machine),
                    abi::PLATFORM_NAME_BYTES,
                )
                .await,
            );

            let cleanup_result = temp.free(&mut pad.trampoline).await;
            match (main_result, cleanup_result) {
                (Ok(r), Ok(())) => Ok(r),
                (Err(e), _) => Err(e),
                (Ok(_), Err(e)) => Err(e),
            }
        }
    };
    let cleanup_result = pad.free().await;
    main_result?;
    cleanup_result?;
    Ok(())
}

/// brk() is emulated using mmap because we can't change the host kernel's per
/// process brk pointer from our loader without extra privileges.
async fn do_brk<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    new_brk: VPtr,
) -> Result<VPtr, Errno> {
    if new_brk.0 != 0 {
        let old_brk = stopped_task.task.task_data.mm.brk;
        let brk_start = stopped_task.task.task_data.mm.brk_start;
        let old_brk_page = VPage::round_up(brk_start.ptr().max(old_brk));
        let new_brk_page = VPage::round_up(brk_start.ptr().max(new_brk));

        if new_brk_page != old_brk_page {
            let mut tr = Trampoline::new(stopped_task);

            if new_brk_page == brk_start {
                tr.munmap(&(brk_start..old_brk_page)).await?;
            } else if old_brk_page == brk_start {
                tr.mmap(
                    &MappedPages::anonymous(brk_start..new_brk_page),
                    &RemoteFd::invalid(),
                    &MemFlags::rw(),
                    abi::MAP_ANONYMOUS,
                )
                .await?;
            } else {
                tr.mremap(
                    &(brk_start..old_brk_page),
                    new_brk_page.ptr().0 - brk_start.ptr().0,
                )
                .await?;
            }
        }
        stopped_task.task.task_data.mm.brk = brk_start.ptr().max(new_brk);
    }
    Ok(stopped_task.task.task_data.mm.brk)
}
