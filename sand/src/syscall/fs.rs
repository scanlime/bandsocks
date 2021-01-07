use crate::{
    process::task::StoppedTask,
    protocol::{Errno, FileStat, FollowLinks, FromTask, ToTask, VFile, VPtr},
    remote::{file::RemoteFd, trampoline::Trampoline},
    syscall::result::SyscallResult,
};

pub async fn do_getdents<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
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

pub async fn do_fstat<'q, 's, 't>(
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

pub async fn do_close<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    fd: RemoteFd,
) -> Result<(), Errno> {
    // Note that the fd will be closed even if close() also reports an error
    stopped_task.task.task_data.file_table.close(&fd);
    let mut tr = Trampoline::new(stopped_task);
    fd.close(&mut tr).await
}
