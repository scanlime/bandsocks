use crate::{
    errors::IPCError,
    process::Process,
    sand,
    sand::protocol::{
        deserialize, serialize, Errno, FileQueued, FromSand, MessageFromSand, MessageToSand,
        ToSand, VPid, BUFFER_SIZE,
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
            let mut buffer = [0; BUFFER_SIZE];

            while let Ok(len) = self.stream.read(&mut buffer[..]).await {
                if len <= 0 {
                    break;
                }
                log::trace!("ipc read {}", len);
                let mut offset = 0;
                while offset < len {
                    let (message, bytes_used) = deserialize(&buffer[offset..len])?;
                    self.handle_message(message).await?;
                    offset += bytes_used;
                }
            }
            log::warn!("ipc server is exiting");
            Ok(())
        })
    }

    fn enqueue_file(&mut self, fd: &impl AsRawFd) -> Result<(), IPCError> {
        self.stream.enqueue(fd)?;
        Ok(())
    }

    async fn send_message(&mut self, message: &MessageToSand) -> Result<(), IPCError> {
        log::info!("<{:x?}", message);

        let mut buffer = [0; BUFFER_SIZE];
        let len = serialize(&mut buffer, message).unwrap();
        self.stream.write_all(&buffer[0..len]).await?;
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
                    assert!(self.process_table.insert(message.task, process).is_none());
                    self.reply(&message, ToSand::OpenProcessReply).await?;
                }
            }

            FromSand::SysAccess(access) => match self.process_table.get_mut(&message.task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(mut process) => {
                    let path = process.read_string(access.path)?;
                    log::info!("{:x?} sys_access({:?})", message.task, path);
                    self.reply(&message, ToSand::SysAccessReply(Err(Errno(-libc::ENOENT))))
                        .await?;
                }
            },

            FromSand::SysOpen(access, flags) => match self.process_table.get_mut(&message.task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(mut process) => {
                    let path = process.read_string(access.path)?;
                    log::info!("{:x?} sys_open({:?})", message.task, path);
                    let file = std::fs::File::open("/dev/null")?;
                    self.enqueue_file(&file)?;
                    self.reply(&message, ToSand::SysOpenReply(Ok(FileQueued())))
                        .await?;
                    drop(file);
                }
            },

            FromSand::SysKill(_vpid, _signal) => match self.process_table.get_mut(&message.task) {
                None => Err(IPCError::WrongProcessState)?,
                Some(mut process) => {
                    self.reply(&message, ToSand::SysKillReply(Ok(()))).await?;
                }
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
