use crate::{
    container::ExitStatus,
    errors::RuntimeError,
    filesystem::{storage::FileStorage, vfs::Filesystem},
    process::{Process, ProcessStatus},
    sand,
    sand::protocol::{
        buffer, buffer::IPCBuffer, exit::*, Errno, FileStat, FromTask, MessageFromSand,
        MessageToSand, SysFd, ToTask, TracerSettings, VFile, VPid, MEMFD_TEMP_NAME,
    },
    taskcall,
};
use fd_queue::{tokio::UnixStream, EnqueueFd};
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    fs::File,
    io::Write,
    os::{
        raw::c_int,
        unix::{io::AsRawFd, prelude::RawFd},
    },
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

struct SysFdStd(SysFd);

impl AsRawFd for SysFdStd {
    fn as_raw_fd(&self) -> RawFd {
        self.0 .0 as c_int
    }
}

async fn send_message(
    stream: &mut UnixStream,
    message: &MessageToSand,
) -> Result<(), RuntimeError> {
    log::debug!("<{:x?}", message);

    let mut buffer = IPCBuffer::new();
    buffer.push_back(message)?;
    for file in buffer.as_slice().files {
        stream.enqueue(&SysFdStd(*file))?;
    }
    stream.write_all(buffer.as_slice().bytes).await?;
    stream.flush().await?;
    Ok(())
}

fn memfd_from_bytes(bytes: &[u8]) -> Result<File, RuntimeError> {
    let name = MEMFD_TEMP_NAME;
    let name = CStr::from_bytes_with_nul(name).unwrap().to_str().unwrap();
    let mut file = memfd::MemfdOptions::default().create(name)?.into_file();
    file.write_all(bytes)?;
    Ok(file)
}

impl IPCServer {
    pub async fn new<T: AsRawFd>(
        filesystem: Filesystem,
        storage: FileStorage,
        args_socket: &T,
        tracer_settings: TracerSettings,
    ) -> Result<Self, RuntimeError> {
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
                tracer_settings,
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

    pub fn task(mut self) -> JoinHandle<Result<ExitStatus, RuntimeError>> {
        task::spawn(async move {
            let result = self.task_message_loop().await;
            log::trace!("task_message_loop -> {:?}", result);
            self.task_finalize().await?;
            result
        })
    }

    pub async fn task_message_loop(&mut self) -> Result<ExitStatus, RuntimeError> {
        let mut buffer = IPCBuffer::new();
        loop {
            let available = buffer.begin_fill();
            match self.stream.read(available.bytes).await? {
                len if len > 0 => {
                    log::trace!("available={} len={}", available.bytes.len(), len);
                    buffer.commit_fill(len, 0)
                }
                _ => return Err(RuntimeError::Disconnected),
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

    pub async fn task_finalize(self) -> Result<(), RuntimeError> {
        log::trace!("task_finalize begin");
        let output = self.tracer.wait_with_output().await?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::trace!("task_finalize ending");
        if output.status.success() {
            assert_eq!(stderr, "");
            Ok(())
        } else {
            let stderr = stderr.into_owned();
            let status = output.status;
            if status.code() == Some(EXIT_PANIC as i32) {
                Err(RuntimeError::SandPanic { stderr })
            } else if status.code() == Some(EXIT_DISCONNECTED as i32) {
                Err(RuntimeError::SandReportsDisconnect { stderr })
            } else if status.code() == Some(EXIT_IO_ERROR as i32) {
                Err(RuntimeError::SandIOError { stderr })
            } else if status.code() == Some(EXIT_OUT_OF_MEM as i32) {
                Err(RuntimeError::SandOutOfMem { stderr })
            } else {
                Err(RuntimeError::SandUnexpectedStatus { status, stderr })
            }
        }
    }

    pub async fn send_message(&mut self, message: &MessageToSand) -> Result<(), RuntimeError> {
        send_message(&mut self.stream, message).await
    }

    async fn handle_message(
        &mut self,
        message: &MessageFromSand,
    ) -> Result<Option<ExitStatus>, RuntimeError> {
        log::debug!(">{:x?}", message);
        match message {
            MessageFromSand::Task { task, op } => self.handle_task_message(*task, op).await,
        }
    }

    async fn task_reply(
        &mut self,
        task: VPid,
        result: Result<(), Errno>,
    ) -> Result<Option<ExitStatus>, RuntimeError> {
        self.send_message(&MessageToSand::Task {
            task,
            op: ToTask::Reply(result),
        })
        .await?;
        Ok(None)
    }

    async fn task_stat_reply(
        &mut self,
        task: VPid,
        result: Result<(VFile, FileStat), Errno>,
    ) -> Result<Option<ExitStatus>, RuntimeError> {
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
    ) -> Result<Option<ExitStatus>, RuntimeError> {
        // SysFd does not own the underlying file, which must remain allocated until the
        // outgoing message has been flushed.
        let (_storage, reply) = match result {
            Err(e) => (None, Err(e)),
            Ok(vfile) => match self.filesystem.open_storage(&self.storage, &vfile).await {
                Err(e) => (None, Err(e.into())),
                Ok(file) => {
                    let sys_fd = SysFd(file.as_raw_fd() as u32);
                    (Some(file), Ok((vfile, sys_fd)))
                }
            },
        };
        self.send_message(&MessageToSand::Task {
            task,
            op: ToTask::FileReply(reply),
        })
        .await?;
        Ok(None)
    }

    async fn task_bytes_reply(
        &mut self,
        task: VPid,
        result: Result<&[u8], Errno>,
    ) -> Result<Option<ExitStatus>, RuntimeError> {
        let (_storage, reply) = match result {
            Err(e) => (None, Err(e)),
            Ok(bytes) => match memfd_from_bytes(bytes) {
                Err(_) => (None, Err(Errno(-libc::EFAULT))),
                Ok(file) => {
                    let sys_fd = SysFd(file.as_raw_fd() as u32);
                    (Some(file), Ok((sys_fd, bytes.len())))
                }
            },
        };
        self.send_message(&MessageToSand::Task {
            task,
            op: ToTask::BytesReply(reply),
        })
        .await?;
        Ok(None)
    }

    async fn task_cstring_reply(
        &mut self,
        task: VPid,
        result: Result<CString, Errno>,
    ) -> Result<Option<ExitStatus>, RuntimeError> {
        let result = match &result {
            Err(e) => Err(*e),
            Ok(cstring) => Ok(cstring.as_bytes_with_nul()),
        };
        self.task_bytes_reply(task, result).await
    }

    async fn handle_task_message(
        &mut self,
        task: VPid,
        op: &FromTask,
    ) -> Result<Option<ExitStatus>, RuntimeError> {
        match op {
            FromTask::Log(level, message) => {
                sand::task_log(task, *level, message.clone());
                Ok(None)
            }

            FromTask::OpenProcess(sys_pid) => {
                if self.process_table.contains_key(&task) {
                    Err(RuntimeError::WrongProcessState)
                } else {
                    let process = Process::open(
                        *sys_pid,
                        &self.tracer,
                        ProcessStatus {
                            current_dir: Filesystem::root().clone(),
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

            FromTask::GetWorkingDir => match self.process_table.get_mut(&task) {
                None => Err(RuntimeError::WrongProcessState)?,
                Some(process) => {
                    let result = taskcall::get_working_dir(process, &self.filesystem).await;
                    self.task_cstring_reply(task, result).await
                }
            },

            FromTask::ChangeWorkingDir(path) => match self.process_table.get_mut(&task) {
                None => Err(RuntimeError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::change_working_dir(process, &self.filesystem, path).await;
                    self.task_reply(task, result).await
                }
            },

            FromTask::ReadLink(path) => match self.process_table.get_mut(&task) {
                None => Err(RuntimeError::WrongProcessState)?,
                Some(process) => {
                    let result = taskcall::readlink(process, &self.filesystem, path).await;
                    self.task_cstring_reply(task, result).await
                }
            },

            FromTask::FileStat {
                file,
                path,
                follow_links,
            } => match self.process_table.get_mut(&task) {
                None => Err(RuntimeError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_stat(process, &self.filesystem, file, path, follow_links)
                            .await;
                    self.task_stat_reply(task, result).await
                }
            },

            FromTask::FileAccess { dir, path, mode } => match self.process_table.get_mut(&task) {
                None => Err(RuntimeError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_open(process, &self.filesystem, dir, path, 0, *mode)
                            .await
                            .map(|_| ());
                    self.task_reply(task, result).await
                }
            },

            FromTask::FileOpen {
                dir,
                path,
                flags,
                mode,
            } => match self.process_table.get_mut(&task) {
                None => Err(RuntimeError::WrongProcessState)?,
                Some(process) => {
                    let result =
                        taskcall::file_open(process, &self.filesystem, dir, path, *flags, *mode)
                            .await;
                    self.task_file_reply(task, result).await
                }
            },

            FromTask::ProcessKill(_vpid, _signal) => match self.process_table.get_mut(&task) {
                None => Err(RuntimeError::WrongProcessState)?,
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
