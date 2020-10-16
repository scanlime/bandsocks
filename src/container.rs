use crate::{
    client::Client,
    errors::{IPCError, ImageError, RuntimeError, VFSError},
    filesystem::{mmap::MapRef, vfs::Filesystem},
    image::Image,
    ipcserver::IPCServer,
    Reference,
};
use osstrtools::OsStrTools;
use std::{
    collections::BTreeMap,
    default::Default,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::task::JoinHandle;

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

        Ok(Container::startup(filesystem, argv, env, dir)?)
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
    filesystem: Filesystem,
    dir: PathBuf,
    argv: Vec<OsString>,
    env: BTreeMap<OsString, OsString>,
    ipc_join: JoinHandle<Result<(), IPCError>>,
}

impl Container {
    pub fn new() -> ContainerBuilder {
        Default::default()
    }

    pub async fn wait(self) -> Result<(), RuntimeError> {
        self.ipc_join.await??;
        Ok(())
    }

    pub async fn pull(image_reference: &Reference) -> Result<ContainerBuilder, ImageError> {
        let mut builder = Container::new();
        builder.image(&Client::new()?.pull(image_reference).await?);
        Ok(builder)
    }

    fn startup(
        filesystem: Filesystem,
        argv: Vec<OsString>,
        env: BTreeMap<OsString, OsString>,
        dir: PathBuf,
    ) -> Result<Container, RuntimeError> {
        let ipc_join = IPCServer::new()?.task();

        Ok(Container {
            filesystem,
            argv,
            env,
            dir,
            ipc_join,
        })
    }

    fn program_image_for_execp(&self, argv0: &OsStr) -> Result<MapRef, RuntimeError> {
        // Following the convention of execvp() and friends, the PATH is searched
        // if and only if argv0 has no slash characters in it.

        let absolute_paths = if argv0.contains("/") {
            vec![Path::new(argv0).to_path_buf()]
        } else {
            let env_path = self.env.get(&OsString::from("PATH"));
            env_path
                .as_ref()
                .map(|env_path| env_path.split(":"))
                .unwrap_or_else(Vec::new)
                .iter()
                .map(|env_path_item| Path::new(env_path_item).join(argv0))
                .collect()
        };
        log::info!("searching paths, {:?}", absolute_paths);
        match absolute_paths
            .iter()
            .map(|path_buf| self.filesystem.get_file_data(path_buf))
            .skip_while(|result| {
                if let Err(VFSError::NotFound) = result {
                    true
                } else {
                    false
                }
            })
            .next()
        {
            None => Err(VFSError::NotFound)?,
            Some(Err(other)) => Err(other)?,
            Some(Ok(mmap)) => Ok(mmap),
        }
    }
}