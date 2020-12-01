use crate::{
    container::ExitStatus,
    errors::IPCError,
    filesystem::{
        storage::FileStorage,
        vfs::{Filesystem, VFile},
    },
    process::{Process, ProcessStatus},
    sand,
    sand::protocol::{
        buffer, buffer::IPCBuffer, Errno, FileStat, FromTask, MessageFromSand, MessageToSand,
        SysFd, ToTask, VPid,
    },
    taskcall,
};
use fd_queue::{tokio::UnixStream, EnqueueFd};
use std::{
    collections::HashMap,
    os::unix::{io::AsRawFd, prelude::RawFd},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
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
    pub async fn new<T: AsRawFd>(
        filesystem: Filesystem,
        storage: FileStorage,
        args_socket: &T,
    ) -> Result<Self, IPCError> {
        let (mut server_socket, child_socket) = UnixStream::pair()?;
        clear_close_on_exec_flag(child_socket.as_raw_fd());

        let args_fd = args_socket.as_raw_fd();
        assert_eq!(0, unsafe { libc::fcntl(args_fd, libc::F_SETFL, 0) });
        let args_fd = SysFd(args_fd as u32);

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

        let mut command: Command = sand::command(child_socket.as_raw_fd())?.into();
        let tracer = command.spawn()?;

        Ok(IPCServer {
            filesystem,
            storage,
            tracer,
            stream: server_socket,
            process_table: HashMap::new(),
        })
    }

    pub fn task(mut self) -> JoinHandle<Result<ExitStatus, IPCError>> {
        task::spawn(async move {
            let result = self.task_message_loop().await;
            log::trace!("task_message_loop -> {:?}", result);
            self.task_finalize().await?;
            result
        })
    }

    pub async fn task_message_loop(&mut self) -> Result<ExitStatus, IPCError> {
        let mut buffer = IPCBuffer::new();
        loop {
            let available = buffer.begin_fill();
            match self.stream.read(available.bytes).await? {
                len if len > 0 => {
                    log::trace!("available={} len={}", available.bytes.len(), len);
                    buffer.commit_fill(len, 0)
                }
                _ => return Err(IPCError::Disconnected),
            }
            while !buffer.is_empty() {
                let message = match buffer.pop_front() {
                    Ok(message) => message,
                    Err(buffer::Error::UnexpectedEnd) => break,
                    Err(err) => return Err(err.into()),
                };
                match self.handle_message(&message).await {
                    Err(err) => {
                        log::error!("{:?} while handling {:?}", err, message);
                        return Err(err);
                    }
                    Ok(Some(exit)) => return Ok(exit),
                    Ok(None) => (),
                }
            }
        }
    }

    pub async fn task_finalize(self) -> Result<(), IPCError> {
        log::trace!("task_finalize begin");
        let output = self.tracer.wait_with_output().await?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::trace!("task_finalize ending");
        if output.status.success() {
            assert_eq!(stderr, "");
            Ok(())
        } else {
            Err(IPCError::SandError {
                status: output.status,
                stderr: stderr.into_owned(),
            })
        }
    }

    pub async fn send_message(&mut self, message: &MessageToSand) -> Result<(), IPCError> {
        send_message(&mut self.stream, message).await
    }

    async fn handle_message(
        &mut self,
        message: &MessageFromSand,
    ) -> Result<Option<ExitStatus>, IPCError> {
        log::debug!(">{:x?}", message);
        match message {
            MessageFromSand::Task { task, op } => self.handle_task_message(*task, op).await,
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
        op: &FromTask,
    ) -> Result<Option<ExitStatus>, IPCError> {
        match op {
            FromTask::Log(level, message) => {
                sand::task_log(task, *level, message.clone());
                Ok(None)
            }

            FromTask::OpenProcess(sys_pid) => {
                if self.process_table.contains_key(&task) {
                    Err(IPCError::WrongProcessState)
                } else {
                    let process = Process::open(
                        *sys_pid,
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
                        taskcall::get_working_dir(process, &self.filesystem, *buffer, *size).await;
                    self.task_size_reply(task, result).await
                }
            },

            FromTask::ChangeWorkingDir(path) => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::change_working_dir(process, &self.filesystem, *path).await;
                    self.task_reply(task, result).await
                }
            },

            FromTask::FileStat { fd, path, nofollow } => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_stat(process, &self.filesystem, *fd, *path, *nofollow).await;
                    self.task_stat_reply(task, result).await
                }
            },

            FromTask::FileAccess { dir, path, mode } => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_access(process, &self.filesystem, *dir, *path, *mode).await;
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
                        taskcall::file_open(process, &self.filesystem, *dir, *path, *flags, *mode)
                            .await;
                    self.task_file_reply(task, result).await
                }
            },

            FromTask::ProcessKill(_vpid, _signal) => match self.process_table.get_mut(&task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(_process) => self.task_reply(task, Ok(())).await,
            },

            FromTask::Exited(exit_code) => Ok(Some(ExitStatus { code: *exit_code })),
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
