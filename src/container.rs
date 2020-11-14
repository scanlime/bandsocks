use crate::{
    errors::{ImageError, RuntimeError},
    filesystem::{storage::FileStorage, vfs::Filesystem},
    image::Image,
    ipcserver::IPCServer,
    registry::Client,
    sand::protocol::{InitArgsHeader, SysFd},
    Reference,
};
use fd_queue::tokio::UnixStream;
use std::{
    collections::BTreeMap,
    default::Default,
    env::split_paths,
    ffi::{OsStr, OsString},
    os::unix::{ffi::OsStrExt, io::AsRawFd},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{io::AsyncWriteExt, task::JoinHandle};

#[derive(Default)]
pub struct ContainerBuilder {
    image: Option<Arc<Image>>,
    arg_list: Vec<OsString>,
    env_list: Vec<EnvBuilder>,
    current_dir: Option<OsString>,
    entrypoint: Option<OsString>,
}

enum EnvBuilder {
    Set(OsString, OsString),
    Remove(OsString),
    Clear,
}

impl ContainerBuilder {
    pub fn spawn(&self) -> Result<Container, RuntimeError> {
        // it might be nice to enforce this at compile-time instead... right now it
        // seemed worth allowing for multiple ways to load images without the
        // types getting too complex.
        let image = match &self.image {
            None => Err(RuntimeError::NoImage)?,
            Some(image) => image.clone(),
        };

        // this is a shallow copy of the image's reference filesystem, which the
        // container can modify
        let filesystem = image.filesystem.clone();
        let storage = image.storage.clone();

        // working directory is the configured one joined with an optional relative or
        // absolute override
        let mut dir = PathBuf::new();
        dir.push(&image.config.config.working_dir);
        if let Some(dir_override) = &self.current_dir {
            dir.push(dir_override);
        }

        // merge the environment, allowing arbitrary overrides to the configured
        // environment
        let mut env = BTreeMap::new();
        for configured_env in &image.config.config.env {
            let mut iter = configured_env.splitn(2, "=");
            if let Some(key) = iter.next() {
                let value = match iter.next() {
                    Some(value) => value,
                    None => "",
                };
                env.insert(OsString::from(key), OsString::from(value));
            }
        }
        for env_override in &self.env_list {
            match env_override {
                EnvBuilder::Clear => {
                    env.clear();
                }
                EnvBuilder::Remove(key) => {
                    env.remove(key);
                }
                EnvBuilder::Set(key, value) => {
                    env.insert(key.clone(), value.clone());
                }
            }
        }

        // merge the command line arguments, allowing an "entry point" binary from
        // either the config or our local overrides, followed by additional
        // "cmd" args that can be taken exactly as configured or replaced
        // entirely with the arguments given to this invocation.
        let mut argv = match &self.entrypoint {
            Some(path) => vec![path.clone()],
            None => match &image.config.config.entrypoint {
                Some(arg_list) => arg_list.iter().map(OsString::from).collect(),
                None => vec![],
            },
        };
        if self.arg_list.is_empty() {
            argv.extend(image.config.config.cmd.iter().map(OsString::from));
        } else {
            argv.extend(self.arg_list.clone());
        }
        if argv.is_empty() {
            Err(RuntimeError::NoEntryPoint)?
        }

        Ok(Container::startup(filesystem, storage, argv, env, dir)?)
    }

    pub fn image(&mut self, image: &Arc<Image>) -> &mut Self {
        self.image = Some(image.clone());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg.as_ref());
        }
        self
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.arg_list.push(arg.as_ref().to_os_string());
        self
    }

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.current_dir = Some(dir.as_ref().as_os_str().to_os_string());
        self
    }

    pub fn entrypoint<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.entrypoint = Some(path.as_ref().as_os_str().to_os_string());
        self
    }

    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env_list.push(EnvBuilder::Set(
            key.as_ref().to_os_string(),
            val.as_ref().to_os_string(),
        ));
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (ref key, ref val) in vars {
            self.env(key, val);
        }
        self
    }

    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Self {
        self.env_list
            .push(EnvBuilder::Remove(key.as_ref().to_os_string()));
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self.env_list.push(EnvBuilder::Clear);
        self
    }
}

#[derive(Debug)]
pub struct Container {
    join: JoinHandle<Result<(), RuntimeError>>,
}

impl Container {
    pub fn new() -> ContainerBuilder {
        Default::default()
    }

    pub async fn wait(self) -> Result<(), RuntimeError> {
        self.join.await?
    }

    pub async fn pull(image_reference: &Reference) -> Result<ContainerBuilder, ImageError> {
        let mut builder = Container::new();
        builder.image(&Client::new()?.pull(image_reference).await?);
        Ok(builder)
    }

    fn startup(
        filesystem: Filesystem,
        storage: FileStorage,
        argv: Vec<OsString>,
        env: BTreeMap<OsString, OsString>,
        dir: PathBuf,
    ) -> Result<Container, RuntimeError> {
        let mut filename = match argv.first() {
            Some(argv0) => PathBuf::from(argv0),
            None => return Err(RuntimeError::NoEntryPoint),
        };

        // the execvpe() behavior here is that path resolution happens if there are no
        // slashes in the original name, i.e. if it is relative and of length
        // one.
        if filename.is_relative() && filename.iter().count() == 1 {
            if let Some(env_paths) = env.get(&OsString::from("PATH")) {
                for env_path in split_paths(env_paths) {
                    let mut buf = PathBuf::from(&dir);
                    buf.push(&env_path);
                    buf.push(&filename);
                    if filesystem.open(&buf).is_ok() {
                        filename = buf;
                    }
                }
            }
        }

        let args_header = InitArgsHeader {
            dir_len: dir.as_os_str().len() + 1,
            filename_len: filename.as_os_str().len() + 1,
            argv_len: argv.iter().map(|s| s.len() + 1).sum::<usize>() + 1,
            envp_len: env
                .iter()
                .map(|(k, v)| k.len() + 1 + v.len() + 1)
                .sum::<usize>()
                + 1,
            arg_count: argv.len(),
            env_count: env.len(),
        };

        log::info!(
            "resolved command filename to {:?} with args={:?}, env={:?}, dir={:?} -> {:?}",
            filename,
            argv,
            env,
            dir,
            args_header
        );

        Ok(Container {
            join: tokio::spawn(async move {
                let (mut args, args_client) = UnixStream::pair()?;
                let client_fd = args_client.as_raw_fd();
                assert_eq!(0, unsafe { libc::fcntl(client_fd, libc::F_SETFL, 0) });
                let client_fd = SysFd(client_fd as u32);
                let ipc_task = IPCServer::new(filesystem, storage, client_fd).await?.task();

                args.write_all(args_header.as_bytes()).await?;

                args.write_all(dir.as_os_str().as_bytes()).await?;
                args.write_all(b"\0").await?;

                args.write_all(filename.as_os_str().as_bytes()).await?;
                args.write_all(b"\0").await?;

                for arg in argv {
                    args.write_all(arg.as_os_str().as_bytes()).await?;
                    args.write_all(b"\0").await?;
                }
                args.write_all(b"\0").await?;

                for (k, v) in env.iter() {
                    args.write_all(k.as_os_str().as_bytes()).await?;
                    args.write_all(b"=").await?;
                    args.write_all(v.as_os_str().as_bytes()).await?;
                    args.write_all(b"\0").await?;
                }
                args.write_all(b"\0").await?;

                args.flush().await?;
                ipc_task.await??;

                Ok(())
            }),
        })
    }
}
