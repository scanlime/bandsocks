use crate::{
    errors::IPCError,
    filesystem::{
        storage::FileStorage,
        vfs::{Filesystem, VFile},
    },
    process::{Process, ProcessStatus},
    sand,
    sand::protocol::{
        buffer::IPCBuffer, Errno, FileStat, FromTask, MessageFromSand, MessageToSand, SysFd,
        ToTask, VPid,
    },
    taskcall,
};
use fd_queue::{tokio::UnixStream, EnqueueFd};
use pentacle::SealedCommand;
use std::{
    collections::HashMap,
    io::Cursor,
    os::unix::{
        io::AsRawFd,
        prelude::RawFd,
        process::{CommandExt, ExitStatusExt},
    },
    process::{Child, ExitStatus},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task,
    task::JoinHandle,
};

pub struct IPCServer {
    filesystem: Filesystem,
    storage: FileStorage,
    tracer: Child,
    stream: UnixStream,
    process_table: HashMap<VPid, Process>,
}

async fn send_message(stream: &mut UnixStream, message: &MessageToSand) -> Result<(), IPCError> {
    log::debug!("<{:x?}", message);

    let mut buffer = IPCBuffer::new();
    buffer.push_back(message)?;
    for file in buffer.as_slice().files {
        stream.enqueue(file)?;
    }
    stream.write_all(buffer.as_slice().bytes).await?;
    stream.flush().await?;
    Ok(())
}

impl IPCServer {
    pub async fn new(
        filesystem: Filesystem,
        storage: FileStorage,
        args_fd: SysFd,
    ) -> Result<Self, IPCError> {
        let (mut server_socket, child_socket) = UnixStream::pair()?;
        clear_close_on_exec_flag(child_socket.as_raw_fd());

        // Queue the init message before running the sand process. It will exit early if
        // it starts up idle.
        send_message(
            &mut server_socket,
            &MessageToSand::Init {
                args: args_fd,
                max_log_level: sand::max_log_level(),
            },
        )
        .await?;

        let mut sand_bin = Cursor::new(sand::PROGRAM_DATA);
        let mut cmd = SealedCommand::new(&mut sand_bin).unwrap();

        // The stage 1 process requires these specific args and env.
        cmd.arg0("sand");
        cmd.env_clear();
        cmd.env("FD", child_socket.as_raw_fd().to_string());

        Ok(IPCServer {
            filesystem,
            storage,
            tracer: cmd.spawn()?,
            stream: server_socket,
            process_table: HashMap::new(),
        })
    }

    pub fn task(mut self) -> JoinHandle<Result<ExitStatus, IPCError>> {
        task::spawn(async move {
            let mut buffer = IPCBuffer::new();
            loop {
                buffer.reset();
                unsafe { buffer.set_len(buffer.byte_capacity(), 0) };
                match self.stream.read(buffer.as_slice_mut().bytes).await? {
                    len if len > 0 => unsafe { buffer.set_len(len, 0) },
                    _ => return Err(IPCError::Disconnected),
                }
                while !buffer.is_empty() {
                    let message = buffer.pop_front()?;
                    if let Some(exit) = self.handle_message(message).await? {
                        return Ok(exit);
                    }
                }
            }
        })
    }

    pub async fn send_message(&mut self, message: &MessageToSand) -> Result<(), IPCError> {
        send_message(&mut self.stream, message).await
    }

    async fn handle_message(
        &mut self,
        message: MessageFromSand,
    ) -> Result<Option<ExitStatus>, IPCError> {
        log::debug!(">{:x?}", message);
        match message {
            MessageFromSand::Task { task, op } => self.handle_task_message(task, op).await,
        }
    }

    async fn task_reply(
        &mut self,
        task: VPid,
        result: Result<(), Errno>,
    ) -> Result<Option<ExitStatus>, IPCError> {
        self.send_message(&MessageToSand::Task {
            task,
            op: ToTask::Reply(result),
        })
        .await?;
        Ok(None)
    }

    async fn task_size_reply(
        &mut self,
        task: VPid,
        result: Result<usize, Errno>,
    ) -> Result<Option<ExitStatus>, IPCError> {
        self.send_message(&MessageToSand::Task {
            task,
            op: ToTask::SizeReply(result),
        })
        .await?;
        Ok(None)
    }

    async fn task_stat_reply(
        &mut self,
        task: VPid,
        result: Result<FileStat, Errno>,
    ) -> Result<Option<ExitStatus>, IPCError> {
        self.send_message(&MessageToSand::Task {
            task,
            op: ToTask::FileStatReply(result),
        })
        .await?;
        Ok(None)
    }

    async fn task_file_reply(
        &mut self,
        task: VPid,
        result: Result<VFile, Errno>,
    ) -> Result<Option<ExitStatus>, IPCError> {
        // SysFd does not own the underlying file, which must remain allocated until the
        // outgoing message has been flushed.
        let storage = match result {
            Err(e) => Err(e),
            Ok(vfile) => match self.filesystem.vfile_storage(&self.storage, &vfile).await {
                Ok(file) => Ok(file),
                Err(e) => Err(Errno(-e.to_errno())),
            },
        };
        let sys_fd = match &storage {
            Err(e) => Err(*e),
            Ok(file) => Ok(SysFd(file.as_raw_fd() as u32)),
        };
        self.send_message(&MessageToSand::Task {
            task,
            op: ToTask::FileReply(sys_fd),
        })
        .await?;
        Ok(None)
    }

    async fn handle_task_message(
        &mut self,
        task: VPid,
        op: FromTask,
    ) -> Result<Option<ExitStatus>, IPCError> {
        match op {
            FromTask::Log(level, message) => {
                sand::task_log(task, level, message);
                Ok(None)
            }

            FromTask::OpenProcess(sys_pid) => {
                if self.process_table.contains_key(&task) {
                    Err(IPCError::WrongProcessState)
                } else {
                    let process = Process::open(
                        sys_pid,
                        &self.tracer,
                        ProcessStatus {
                            current_dir: self.filesystem.open_root(),
                        },
                    )?;
                    let handle = process.to_handle();
                    assert!(self.process_table.insert(task, process).is_none());
                    self.send_message(&MessageToSand::Task {
                        task,
                        op: ToTask::OpenProcessReply(handle),
                    })
                    .await?;
                    Ok(None)
                }
            }

            FromTask::GetWorkingDir(buffer, size) => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::get_working_dir(process, &self.filesystem, buffer, size).await;
                    self.task_size_reply(task, result).await
                }
            },

            FromTask::ChangeWorkingDir(path) => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::change_working_dir(process, &self.filesystem, path).await;
                    self.task_reply(task, result).await
                }
            },

            FromTask::FileStat { fd, path, nofollow } => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_stat(process, &self.filesystem, fd, path, nofollow).await;
                    self.task_stat_reply(task, result).await
                }
            },

            FromTask::FileAccess { dir, path, mode } => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_access(process, &self.filesystem, dir, path, mode).await;
                    self.task_reply(task, result).await
                }
            },

            FromTask::FileOpen {
                dir,
                path,
                flags,
                mode,
            } => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_open(process, &self.filesystem, dir, path, flags, mode)
                            .await;
                    self.task_file_reply(task, result).await
                }
            },

            FromTask::ProcessKill(_vpid, _signal) => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(_process) => self.task_reply(task, Ok(())).await,
            },

            FromTask::Exited(exit_code) => Ok(Some(ExitStatus::from_raw(exit_code))),
        }
    }
}

fn clear_close_on_exec_flag(fd: RawFd) {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0);
    let flags = flags & !libc::FD_CLOEXEC;
    let result = unsafe { libc::fcntl(fd, libc::F_SETFD, flags) };
    assert_eq!(result, 0);
}
