//! Sandboxed subprocesses with a virtual filesystem

mod builder;

pub use builder::ContainerBuilder;

use crate::{
    errors::{ImageError, RuntimeError},
    filesystem::{storage::FileStorage, vfs::Filesystem},
    image::{Image, ImageName},
    ipcserver::IPCServer,
    registry::RegistryClient,
    sand::protocol::{InitArgsHeader, TracerSettings},
};
use std::{borrow::Cow, ffi::CString, fmt, io, os::unix::net::UnixStream, sync::Arc, thread};
use tokio::{
    io::{AsyncWriteExt, BufWriter},
    task,
    task::JoinHandle,
};

/// A running container
///
/// Roughly analogous to [std::process::Child], but for a sandbox container.
#[derive(Debug)]
pub struct Container {
    pub stdin: Option<UnixStream>,
    pub stdout: Option<UnixStream>,
    pub stderr: Option<UnixStream>,
    join: JoinHandle<Result<ExitStatus, RuntimeError>>,
}

/// Status of an exited container
///
/// Much like [std::process::ExitStatus]
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

/// Output from an exited container
///
/// Much like [std::process::Output]
#[derive(Clone, Eq, PartialEq)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    /// Convert the stdout to utf8 if possible
    ///
    /// Equivalent to `String::from_utf8_lossy(output.stdout)`
    pub fn stdout_str(&self) -> Cow<str> {
        String::from_utf8_lossy(&self.stdout)
    }

    /// Convert the stderr to utf8 if possible
    ///
    /// Equivalent to `String::from_utf8_lossy(output.stderr)`
    pub fn stderr_str(&self) -> Cow<str> {
        String::from_utf8_lossy(&self.stderr)
    }
}

impl fmt::Debug for Output {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Output")
            .field("status", &self.status)
            .field("stdout", &self.stdout_str())
            .field("stderr", &self.stderr_str())
            .finish()
    }
}

fn expect_broken_pipe(result: io::Result<()>) -> io::Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
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
    /// This is equivalent to using [RegistryClient::pull()] followed by
    /// [Container::new()], which as an alternative allows using custom
    /// settings for [RegistryClient].
    pub async fn pull(name: &ImageName) -> Result<ContainerBuilder, ImageError> {
        Container::new(RegistryClient::new()?.pull(name).await?)
    }

    /// Wait for the container to finish running, if necessary, and return its
    /// exit status.
    pub async fn wait(self) -> Result<ExitStatus, RuntimeError> {
        log::trace!("wait starting");
        let result = self.join.await?;
        log::trace!("wait complete -> {:?}", result);
        result
    }

    /// Wait for the container to finish running, while connecting it to stdio
    ///
    /// Any stdio streams which haven't been taken from the [Container] or
    /// overridden with [ContainerBuilder] will be forwarded to/from their real
    /// equivalents until the container exits.
    ///
    /// If stdin is available to forward, we will use a separate thread for
    /// the blocking stdin reads. This thread may continue running after
    /// the container itself exits, since `std`'s stdin reads cannot be
    /// cancelled.
    pub async fn interact(self) -> Result<ExitStatus, RuntimeError> {
        log::trace!("interact starting");
        if let Some(mut stream) = self.stdin {
            let _ = thread::Builder::new()
                .name("stdin".to_string())
                .spawn(move || {
                    let _ = io::copy(&mut io::stdin(), &mut stream);
                });
        }

        let stdout = self.stdout;
        let stdout = tokio::spawn(async move {
            if let Some(stream) = stdout {
                let mut stream = tokio::net::UnixStream::from_std(stream)?;
                tokio::io::copy(&mut stream, &mut tokio::io::stdout()).await?;
            }
            Ok::<(), tokio::io::Error>(())
        });
        let stderr = self.stderr;
        let stderr = tokio::spawn(async move {
            if let Some(stream) = stderr {
                let mut stream = tokio::net::UnixStream::from_std(stream)?;
                tokio::io::copy(&mut stream, &mut tokio::io::stderr()).await?;
            }
            Ok::<(), tokio::io::Error>(())
        });

        let status = self.join.await??;
        log::trace!("interact waiting for stdout/stderr");
        let (stdout, stderr) = tokio::join!(stdout, stderr);
        expect_broken_pipe(stdout?)?;
        expect_broken_pipe(stderr?)?;
        log::trace!("interact finished, {:?}", status);
        Ok(status)
    }

    /// Capture the container's output and wait for it to finish
    ///
    /// This will capture stderr and stdout if they have not been
    /// taken from the [Container] or overridden with [ContainerBuilder].
    ///
    /// If stdin has not been taken or overridden, it will be dropped.
    pub async fn output(self) -> Result<Output, RuntimeError> {
        drop(self.stdin);

        fn output_task(stream: Option<UnixStream>) -> JoinHandle<tokio::io::Result<Vec<u8>>> {
            task::spawn(async move {
                let mut buf = Vec::<u8>::new();
                if let Some(stream) = stream {
                    let mut stream = tokio::net::UnixStream::from_std(stream)?;
                    tokio::io::copy(&mut stream, &mut buf).await?;
                }
                Ok(buf)
            })
        }
        let stdout = output_task(self.stdout);
        let stderr = output_task(self.stderr);

        log::trace!("output wait starting");
        let status = self.join.await??;
        let stdout = stdout.await??;
        let stderr = stderr.await??;
        let result = Output {
            status,
            stdout,
            stderr,
        };

        log::trace!("output wait complete -> {:?}", result);
        Ok(result)
    }

    pub(crate) fn exec(
        filesystem: Filesystem,
        storage: FileStorage,
        filename: CString,
        dir: CString,
        argv: Vec<CString>,
        env: Vec<CString>,
        stdio: [Option<UnixStream>; 3],
        tracer_settings: TracerSettings,
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

        let [stdin, stdout, stderr] = stdio;

        Ok(Container {
            stdin,
            stdout,
            stderr,
            join: tokio::spawn(async move {
                let ipc_task = {
                    let (args_local, args_remote) = fd_queue::tokio::UnixStream::pair()?;
                    let mut args_buf = BufWriter::new(args_local);
                    let ipc_task =
                        IPCServer::new(filesystem, storage, &args_remote, tracer_settings)
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
