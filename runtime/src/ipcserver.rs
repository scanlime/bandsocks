use crate::{
    errors::RuntimeError,
    sand,
    sand::protocol::{
        deserialize, serialize, FromSand, MessageFromSand, MessageToSand, ToSand, BUFFER_SIZE,
    },
};
use fd_queue::tokio::UnixStream;
use pentacle::SealedCommand;
use std::{
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
    pub fn new() -> Result<IPCServer, RuntimeError> {
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

    pub fn task(mut self) -> JoinHandle<Result<(), RuntimeError>> {
        task::spawn(async move {
            let mut buffer = [0; BUFFER_SIZE];

            while let Ok(len) = self.stream.read(&mut buffer[..]).await {
                if len <= 0 {
                    break;
                }
                log::trace!("ipc read {}", len);
                let mut offset = 0;
                while offset < len {
                    match deserialize(&buffer[offset..len]) {
                        Err(e) => {
                            log::warn!("failed to deserialize message, {:?}", e);
                            break;
                        }
                        Ok((message, bytes_used)) => {
                            if let Err(e) = self.handle_message(message).await {
                                log::warn!("error while handling ipc message, {:?}", e);
                                break;
                            }
                            offset += bytes_used;
                        }
                    }
                }
            }
            log::warn!("ipc server is exiting");
            Ok(())
        })
    }

    async fn send_message(&mut self, message: &MessageToSand) -> Result<(), RuntimeError> {
        log::info!("<{:?}", message);
        let mut buffer = [0; BUFFER_SIZE];
        let len = serialize(&mut buffer, message).unwrap();
        Ok(self.stream.write_all(&buffer[0..len]).await?)
    }

    async fn handle_message(&mut self, message: MessageFromSand) -> Result<(), RuntimeError> {
        log::info!(">{:?}", message);

        tokio::time::delay_for(std::time::Duration::from_millis(2000)).await;

        match &message.op {
            FromSand::SysAccess(_, _) => self.send_message(&MessageToSand {
                task: message.task,
                op: ToSand::AccessReply(Ok(()))
            }).await?,
            _ => (),
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
