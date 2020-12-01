//! Sandboxed subprocesses with a virtual filesystem

mod builder;

pub use builder::ContainerBuilder;

use crate::{
    errors::{ImageError, RuntimeError},
    filesystem::{storage::FileStorage, vfs::Filesystem},
    image::{Image, ImageName},
    ipcserver::IPCServer,
    registry::Client,
    sand::protocol::InitArgsHeader,
};
use std::{ffi::CString, sync::Arc};
use tokio::{
    io::{AsyncWriteExt, BufWriter},
    task::JoinHandle,
};

/// A running container, analogous to [std::process::Child]
#[derive(Debug)]
pub struct Container {
    join: JoinHandle<Result<ExitStatus, RuntimeError>>,
}

/// Status of an exited container, analogous to [std::process::ExitStatus]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExitStatus {
    pub(crate) code: i32,
}

impl ExitStatus {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    pub fn code(&self) -> Option<i32> {
        Some(self.code)
    }
}

impl Container {
    /// Prepare to run a new container, starting with an [Image] loaded
    pub fn new(image: Arc<Image>) -> Result<ContainerBuilder, ImageError> {
        // Take over the image if nobody else is using it, otherwise
        // make a shallow copy of the filesystem.
        match Arc::try_unwrap(image) {
            Ok(owned) => {
                ContainerBuilder::new(&owned.config.config, owned.filesystem, owned.storage)
            }
            Err(shared) => ContainerBuilder::new(
                &shared.config.config,
                shared.filesystem.clone(),
                shared.storage.clone(),
            ),
        }
    }

    /// Prepare to run a new container, starting with an [ImageName] referencing
    /// a repository server.
    ///
    /// This is equivalent to using [Client::pull()] followed by
    /// [Container::new()], which as an alternative allows using custom
    /// settings for [Client].
    pub async fn pull(name: &ImageName) -> Result<ContainerBuilder, ImageError> {
        Container::new(Client::new()?.pull(name).await?)
    }

    /// Wait for the container to finish running, if necessary, and return its
    /// exit status.
    pub async fn wait(self) -> Result<ExitStatus, RuntimeError> {
        log::trace!("wait starting");
        let result = self.join.await?;
        log::trace!("wait complete -> {:?}", result);
        result
    }

    pub(crate) fn exec(
        filesystem: Filesystem,
        storage: FileStorage,
        filename: CString,
        dir: CString,
        argv: Vec<CString>,
        env: Vec<CString>,
    ) -> Result<Container, RuntimeError> {
        log::debug!(
            "exec file={:?} dir={:?} argv={:?} env={:?}",
            filename,
            dir,
            argv,
            env
        );

        let filename = filename.into_bytes_with_nul();
        let dir = dir.into_bytes_with_nul();
        let argv: Vec<Vec<u8>> = argv.into_iter().map(CString::into_bytes_with_nul).collect();
        let env: Vec<Vec<u8>> = env.into_iter().map(CString::into_bytes_with_nul).collect();

        let args_header = InitArgsHeader {
            dir_len: dir.len(),
            filename_len: filename.len(),
            argv_len: argv.iter().map(Vec::len).sum::<usize>() + 1,
            envp_len: env.iter().map(Vec::len).sum::<usize>() + 1,
            arg_count: argv.len(),
            env_count: env.len(),
        };

        Ok(Container {
            join: tokio::spawn(async move {
                let ipc_task = {
                    let (args_local, args_remote) = fd_queue::tokio::UnixStream::pair()?;
                    let mut args_buf = BufWriter::new(args_local);
                    let ipc_task = IPCServer::new(filesystem, storage, &args_remote)
                        .await?
                        .task();

                    args_buf.write_all(args_header.as_bytes()).await?;
                    args_buf.write_all(&dir).await?;
                    args_buf.write_all(&filename).await?;

                    for bytes in argv {
                        args_buf.write_all(&bytes).await?;
                    }
                    args_buf.write_all(b"\0").await?;

                    for bytes in env {
                        args_buf.write_all(&bytes).await?;
                    }
                    args_buf.write_all(b"\0").await?;

                    args_buf.flush().await?;
                    ipc_task
                };
                Ok(ipc_task.await??)
            }),
        })
    }
}
