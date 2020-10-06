use tokio::task;
use tokio::task::JoinHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::io::Cursor;
use std::os::unix::process::CommandExt;
use pentacle::SealedCommand;
use fd_queue::tokio::UnixStream;
use std::process::Child;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::RawFd;
use crate::sand;
use crate::sand::protocol::{serialize, deserialize, BUFFER_SIZE, MessageFromSand, MessageToSand};
use crate::errors::RuntimeError;

pub struct IPCServer {
    child: Child,
    stream: UnixStream
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
            stream: server_socket
        })
    }

    pub fn task(mut self) -> JoinHandle<Result<(), RuntimeError>> {
        task::spawn(async move {
            let mut buffer = [0; BUFFER_SIZE];

            while let Ok(len) = self.stream.read(&mut buffer[..]).await {
                if len <= 0 {
                    break;
                }
                log::warn!("ipc read {}", len);
                let mut offset = 0;
                while offset < len {
                    match deserialize(&buffer[offset .. len]) {
                        Err(e) => {
                            log::warn!("failed to deserialize message, {:?}", e);
                            break;
                        },
                        Ok((message, bytes_used)) => {
                            self.handle_message(message).await;
                            offset += bytes_used;
                        },
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

    async fn handle_message(&mut self, message: MessageFromSand) {
        log::info!(">{:?}", message);
        self.send_message(&MessageToSand::Nop).await.unwrap();
    }
}

fn clear_close_on_exec_flag(fd: RawFd) {
    let flags = unsafe { libc::fcntl( fd, libc::F_GETFD ) };
    assert!(flags >= 0);
    let flags = flags & !libc::FD_CLOEXEC;
    let result = unsafe { libc::fcntl( fd, libc::F_SETFD, flags ) };
    assert_eq!(result, 0);
}
