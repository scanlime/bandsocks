use crate::{
    errors::IPCError,
    process::Process,
    sand,
    sand::protocol::{
        buffer::IPCBuffer, Errno, FromSand, MessageFromSand, MessageToSand, SysFd, ToSand, VPid,
    },
};
use fd_queue::{tokio::UnixStream, EnqueueFd};
use pentacle::SealedCommand;
use std::{
    collections::HashMap,
    io::Cursor,
    os::unix::{io::AsRawFd, prelude::RawFd, process::CommandExt},
    process::Child,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task,
    task::JoinHandle,
};

pub struct IPCServer {
    tracer: Child,
    stream: UnixStream,
    process_table: HashMap<VPid, Process>,
}

impl IPCServer {
    pub fn new() -> Result<IPCServer, IPCError> {
        let (server_socket, child_socket) = UnixStream::pair()?;
        clear_close_on_exec_flag(child_socket.as_raw_fd());

        let mut sand_bin = Cursor::new(sand::PROGRAM_DATA);
        let mut cmd = SealedCommand::new(&mut sand_bin).unwrap();

        // The stage 1 process requires these specific args and env.
        cmd.arg0("sand");
        cmd.env_clear();
        cmd.env("FD", child_socket.as_raw_fd().to_string());

        Ok(IPCServer {
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

    async fn send_message(&mut self, message: &MessageToSand) -> Result<(), IPCError> {
        log::info!("<{:x?}", message);

        let mut buffer = IPCBuffer::new();
        buffer.push_back(message)?;
        for file in buffer.as_slice().files {
            self.stream.enqueue(file)?;
        }
        self.stream.write_all(buffer.as_slice().bytes).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn reply(&mut self, message: &MessageFromSand, op: ToSand) -> Result<(), IPCError> {
        self.send_message(&MessageToSand {
            task: message.task,
            op,
        })
        .await
    }

    async fn handle_message(&mut self, message: MessageFromSand) -> Result<(), IPCError> {
        log::info!(">{:x?}", message);

        match &message.op {
            FromSand::OpenProcess(sys_pid) => {
                if self.process_table.contains_key(&message.task) {
                    Err(IPCError::WrongProcessState)?;
                } else {
                    let process = Process::open(*sys_pid, &self.tracer)?;
                    let handle = process.to_handle();
                    assert!(self.process_table.insert(message.task, process).is_none());
                    self.reply(&message, ToSand::OpenProcessReply(handle))
                        .await?;
                }
            }

            FromSand::FileAccess(access) => match self.process_table.get_mut(&message.task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(process) => {
                    let path = process.read_string(access.path)?;
                    log::info!("{:x?} sys_access({:?})", message.task, path);
                    self.reply(&message, ToSand::FileAccessReply(Err(Errno(-libc::ENOENT))))
                        .await?;
                }
            },

            FromSand::FileOpen { file, flags: _ } => {
                match self.process_table.get_mut(&message.task) {
                    None => Err(IPCError::WrongProcessState)?,
                    Some(process) => {
                        let path = process.read_string(file.path)?;
                        log::info!("{:x?} sys_open({:?})", message.task, path);
                        let backing_file = std::fs::File::open("/dev/null")?;
                        let fd = SysFd(backing_file.as_raw_fd() as u32);
                        self.reply(&message, ToSand::FileOpenReply(Ok(fd))).await?;
                        drop(backing_file);
                    }
                }
            }

            FromSand::ProcessKill(_vpid, _signal) => {
                match self.process_table.get_mut(&message.task) {
                    None => Err(IPCError::WrongProcessState)?,
                    Some(_process) => {
                        self.reply(&message, ToSand::ProcessKillReply(Ok(())))
                            .await?;
                    }
                }
            }
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
