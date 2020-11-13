use crate::{
    errors::IPCError,
    filesystem::vfs::Filesystem,
    process::{Process, ProcessStatus},
    sand,
    sand::protocol::{
        buffer::IPCBuffer, Errno, FileBacking, FromTask, MessageFromSand, MessageToSand, SysFd,
        ToTask, VPid,
    },
};
use fd_queue::{tokio::UnixStream, EnqueueFd};
use pentacle::SealedCommand;
use std::{
    collections::HashMap,
    io::Cursor,
    os::unix::{io::AsRawFd, prelude::RawFd, process::CommandExt},
    path::Path,
    process::Child,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task,
    task::JoinHandle,
};

pub struct IPCServer {
    filesystem: Filesystem,
    tracer: Child,
    stream: UnixStream,
    process_table: HashMap<VPid, Process>,
}

async fn send_message(
    stream: &mut UnixStream,
    message: &MessageToSand,
) -> Result<(), IPCError> {
    log::info!("<{:x?}", message);

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
    pub async fn new(filesystem: Filesystem, args_fd: SysFd) -> Result<Self, IPCError> {
        let (mut server_socket, child_socket) = UnixStream::pair()?;
        clear_close_on_exec_flag(child_socket.as_raw_fd());

        // Queue the init message before running the sand process. It will exit early if
        // it starts up idle.
        send_message(&mut server_socket, &MessageToSand::Init { args: args_fd }).await?;

        let mut sand_bin = Cursor::new(sand::PROGRAM_DATA);
        let mut cmd = SealedCommand::new(&mut sand_bin).unwrap();

        // The stage 1 process requires these specific args and env.
        cmd.arg0("sand");
        cmd.env_clear();
        cmd.env("FD", child_socket.as_raw_fd().to_string());

        Ok(IPCServer {
            filesystem,
            tracer: cmd.spawn()?,
            stream: server_socket,
            process_table: HashMap::new(),
        })
    }

    pub fn task(mut self) -> JoinHandle<Result<(), IPCError>> {
        task::spawn(async move {
            let mut buffer = IPCBuffer::new();
            loop {
                buffer.reset();
                unsafe { buffer.set_len(buffer.byte_capacity(), 0) };
                match self.stream.read(buffer.as_slice_mut().bytes).await? {
                    len if len > 0 => unsafe { buffer.set_len(len, 0) },
                    _ => {
                        log::warn!("ipc server is exiting");
                        break Ok(());
                    }
                }
                while !buffer.is_empty() {
                    let message = buffer.pop_front()?;
                    self.handle_message(message).await?;
                }
            }
        })
    }

    pub async fn send_message(&mut self, message: &MessageToSand) -> Result<(), IPCError> {
        send_message(&mut self.stream, message).await
    }

    async fn handle_message(&mut self, message: MessageFromSand) -> Result<(), IPCError> {
        log::info!(">{:x?}", message);
        match &message {
            MessageFromSand::Task { task, op } => {
                let reply = self.handle_task_message(task, op).await?;
                self.send_message(&MessageToSand::Task {
                    task: *task,
                    op: reply
                }).await?;
                Ok(())
            }
        }
    }

    async fn handle_task_message(&mut self, task: VPid, op: FromTask) -> Result<ToTask, IPCError> {
        match op {
            FromTask::OpenProcess(sys_pid) => {
                if self.process_table.contains_key(task) {
                    Err(IPCError::WrongProcessState)?;
                } else {
                    let process = Process::open(
                        *sys_pid,
                        &self.tracer,
                        ProcessStatus {
                            current_dir: self.filesystem.open_root(),
                        },
                    )?;
                    let handle = process.to_handle();
                    assert!(self.process_table.insert(*task, process).is_none());
                    Ok(ToTask::OpenProcessReply(handle))
                }
            }

            FromTask::ChDir(path) => match self.process_table.get_mut(task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => Ok(ToTask::Reply({
                    let path = process.read_string(*path).map_err(|_| Errno(-libc::EFAULT))?;
                    Ok(ToTask::Reply(Ok(())))
                },
            }

            FromTask::FileAccess { dir, path, mode } => match self.process_table.get_mut(task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => Ok(ToTask::Reply({
                    let path = process.read_string(*path).map_err(|_| Errno(-libc::EFAULT))?;
                    Err(Errno(-libc::ENOENT))
                }
            }

            FromTask::FileOpen {
                dir,
                path,
                flags,
                mode,
            } => match self.process_table.get_mut(task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => Ok(ToTask::FileReply({
                    let path = process.read_string(*path).map_err(|_| Errno(-libc::EFAULT))?;
                    if let Some(_) = dir {
                        log::error!("unimplemented");
                    }
                    let at_dir = Some(&process.status.current_dir);
                    match self.filesystem.open_at(at_dir, Path::new(&path)) {
                        Err(VFSError::DirectoryExpected) => Err(Errno(-libc::ENOTDIR)),
                        Ok(file) => match self.filesystem.map_file(file) {
                            Err(VFSError::DirectoryExpected) => Err(Errno(-libc::ENOTDIR)),
                            Ok(map) => Ok(FileBacking::VFSMapRef {
                                source: SysFd(map.source.as_raw_fd() as u32),
                                offset: map.offset,
                                filesize: map.filesize
                            })
                        }
                    }
                }))
            }

                FromTask::ProcessKill(_vpid, _signal) => match self.process_table.get_mut(task) {
                    None => Err(IPCError::WrongProcessState)?,
                    Some(_process) => {
                        self.send_message(&MessageToSand::Task {
                            task: *task,
                            op: ToTask::Reply(Ok(())),
                        })
                        .await?;
                    }
                },
            },
        }

        Ok(())
    }
}

fn clear_close_on_exec_flag(fd: RawFd) {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0);
    let flags = flags & !libc::FD_CLOEXEC;
    let result = unsafe { libc::fcntl(fd, libc::F_SETFD, flags) };
    assert_eq!(result, 0);
}
