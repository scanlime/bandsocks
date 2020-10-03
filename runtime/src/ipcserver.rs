// This code may not be used for any purpose. Be gay, do crime.

use std::io::Cursor;
use std::os::unix::process::CommandExt;
use pentacle::SealedCommand;
use fd_queue::tokio::UnixStream;
use tokio::task;
use tokio::task::JoinHandle;
use std::process::Child;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::RawFd;
use crate::sand;
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

        cmd.arg0("sand");
        cmd.arg(format!("fd:{}", child_socket.as_raw_fd()));

        Ok(IPCServer {
            child: cmd.spawn()?,
            stream: server_socket
        })
    }

    pub fn task(self) -> JoinHandle<Result<(), RuntimeError>> {
        task::spawn(async move {

            println!("hello from the ipc server");

            Ok(())
        })
    }
}

fn clear_close_on_exec_flag(fd: RawFd) {            
    let flags = unsafe { libc::fcntl( fd, libc::F_GETFD ) };
    assert!(flags >= 0);
    let flags = flags & !libc::FD_CLOEXEC;
    let result = unsafe { libc::fcntl( fd, libc::F_SETFD, flags ) };
    assert_eq!(result, 0);
}
