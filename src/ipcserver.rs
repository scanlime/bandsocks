use crate::{
    errors::IPCError,
    process::Process,
    sand,
    sand::protocol::{
        deserialize, serialize, Errno, FromSand, MessageFromSand, MessageToSand, SysPid, ToSand,
        BUFFER_SIZE,
    },
};
use fd_queue::tokio::UnixStream;
use memmap::MmapMut;
use pentacle::SealedCommand;
use std::{
    fs::OpenOptions,
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
    child: Child,
    stream: UnixStream,
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
            child: cmd.spawn()?,
            stream: server_socket,
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

    async fn send_message(&mut self, message: &MessageToSand) -> Result<(), IPCError> {
        log::info!("<{:x?}", message);

        let mut buffer = [0; BUFFER_SIZE];
        let len = serialize(&mut buffer, message).unwrap();
        Ok(self.stream.write_all(&buffer[0..len]).await?)
    }

    async fn handle_message(&mut self, message: MessageFromSand) -> Result<(), IPCError> {
        log::info!(">{:x?}", message);

        match &message.op {
            FromSand::OpenProcess(sys_pid) => {
                let _process = Process::open(*sys_pid, &self.child)?;
                self.send_message(&MessageToSand {
                    task: message.task,
                    op: ToSand::OpenProcessReply,
                })
                .await?
            }

            FromSand::SysAccess(_access) => {
                self.send_message(&MessageToSand {
                    task: message.task,
                    op: ToSand::SysAccessReply(Ok(())),
                })
                .await?
            }

            FromSand::SysOpen(_access, _flags) => {
                self.send_message(&MessageToSand {
                    task: message.task,
                    op: ToSand::SysOpenReply(Err(Errno(-libc::EINVAL))),
                })
                .await?
            }

            FromSand::SysKill(_vpid, _signal) => {
                self.send_message(&MessageToSand {
                    task: message.task,
                    op: ToSand::SysKillReply(Ok(())),
                })
                .await?
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
