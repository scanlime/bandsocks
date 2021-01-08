use crate::{
    process::task::StoppedTask,
    protocol::{Errno, FileStat, FollowLinks, FromTask, ToTask, VFile, VPtr},
    remote::{file::RemoteFd, trampoline::Trampoline},
    syscall::result::SyscallResult,
};

pub async fn getdents(
    stopped_task: &mut StoppedTask<'_, '_>,
    fd: RemoteFd,
    out_ptr: VPtr,
    len: usize,
) -> SyscallResult {
    let mut tr = Trampoline::new(stopped_task);
    // Directory fds are implemented as sealed memfds full of direntries,
    // so getdents64 simply redirects to read().
    SyscallResult(
        tr.syscall(
            sc::nr::READ,
            &[fd.0 as isize, out_ptr.0 as isize, len as isize],
        )
        .await,
    )
}

pub async fn dup(stopped_task: &mut StoppedTask<'_, '_>, src_fd: RemoteFd) -> Result<RemoteFd, Errno> {
    let mut tr = Trampoline::new(stopped_task);
    let result = tr.syscall(sc::nr::DUP, &[src_fd.0 as isize]).await;
    if result < 0 {
        Err(Errno(result as i32))
    } else {
        let table = &mut stopped_task.task.task_data.file_table;
        let dest_fd = RemoteFd(result as u32);
        table.dup(&src_fd, &dest_fd)?;
        Ok(dest_fd)
    }
}

pub async fn dup2(stopped_task: &mut StoppedTask<'_, '_>, src_fd: RemoteFd, dest_fd: RemoteFd) -> Result<RemoteFd, Errno> {
    let mut tr = Trampoline::new(stopped_task);
    let result = tr.syscall(sc::nr::DUP2, &[src_fd.0 as isize, dest_fd.0 as isize]).await;
    if result < 0 {
        Err(Errno(result as i32))
    } else {
        assert_eq!(result, dest_fd.0 as isize);
        let table = &mut stopped_task.task.task_data.file_table;
        table.dup(&src_fd, &dest_fd)?;
        Ok(dest_fd)
    }
}

pub async fn fstat(
    stopped_task: &mut StoppedTask<'_, '_>,
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

pub async fn close(stopped_task: &mut StoppedTask<'_, '_>, fd: RemoteFd) -> Result<(), Errno> {
    // Note that the fd will be closed even if close() also reports an error
    let table = &mut stopped_task.task.task_data.file_table;
    table.close(&fd);
    let mut tr = Trampoline::new(stopped_task);
    fd.close(&mut tr).await
}
