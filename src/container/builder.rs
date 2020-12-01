use crate::{
    container::Container,
    errors::{ImageError, RuntimeError},
    filesystem::{storage::FileStorage, vfs::Filesystem},
    manifest::ImageConfig,
};
use std::{
    ffi::{CString, NulError, OsStr},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

/// Setup for containers, starting at [Container::new()] and ending with
/// [ContainerBuilder::spawn()]
#[derive(Clone)]
pub struct ContainerBuilder {
    filesystem: Filesystem,
    storage: FileStorage,
    working_dir: CString,
    entrypoint: Vec<CString>,
    cmd_default: Vec<CString>,
    cmd_override: Option<Vec<CString>>,
    env: Vec<CString>,
    arg_error: Result<(), NulError>,
}

async fn new_stdio_socket_wip(fd: u32) -> std::io::Result<std::os::unix::net::UnixStream> {
    let (local, remote) = std::os::unix::net::UnixStream::pair()?;
    tokio::task::spawn(async move {
        log::warn!("stdio wip {}", fd);
        let mut local = tokio::net::UnixStream::from_std(local).unwrap();
        let (mut reader, _writer) = local.split();
        let mut stdout = tokio::io::stdout();
        let out_copy = tokio::io::copy(&mut reader, &mut stdout);
        out_copy.await;
    });
    Ok(remote)
}

impl ContainerBuilder {
    pub(crate) fn new(
        config: &ImageConfig,
        mut filesystem: Filesystem,
        storage: FileStorage,
    ) -> Result<Self, ImageError> {
        let mut writer = filesystem.writer();
        for fd in 0..=2 {
            writer.write_unix_stream_factory(
                std::path::Path::new(&format!("/proc/1/fd/{}", fd)),
                crate::filesystem::vfs::Stat {
                    ..Default::default()
                },
                std::sync::Arc::new(move || {
                    Box::pin(async move {
                        new_stdio_socket_wip(fd)
                            .await
                            .map_err(|_| crate::errors::VFSError::IO)
                    })
                }),
            )?;
        }

        Ok(ContainerBuilder {
            filesystem,
            storage,
            arg_error: Ok(()),
            working_dir: CString::new(config.working_dir.as_bytes())?,
            entrypoint: match &config.entrypoint {
                None => Vec::new(),
                Some(strs) => {
                    let mut result = Vec::new();
                    for s in strs {
                        result.push(CString::new(s.as_bytes())?);
                    }
                    result
                }
            },
            cmd_override: None,
            cmd_default: {
                let mut result = Vec::new();
                for s in &config.cmd {
                    result.push(CString::new(s.as_bytes())?);
                }
                result
            },
            env: {
                let mut result = Vec::new();
                for s in &config.env {
                    result.push(CString::new(s.as_bytes())?);
                }
                result
            },
        })
    }

    /// Start a new [Container] using the settings in this builder
    pub fn spawn(self) -> Result<Container, RuntimeError> {
        self.arg_error?;

        let mut argv = self.entrypoint;
        match self.cmd_override {
            None => argv.extend(self.cmd_default),
            Some(cmd) => argv.extend(cmd),
        };

        // be like execvpe(), doing path resolution if there are no slashes
        let mut filename = argv.first().ok_or(RuntimeError::NoEntryPoint)?.to_owned();
        if !filename.as_bytes().contains(&b'/') {
            if let Some(Some(env_paths)) = env::get(&self.env, b"PATH") {
                for env_path in env_paths.to_bytes().split(|c| *c == b':') {
                    let mut buf = PathBuf::from(OsStr::from_bytes(self.working_dir.as_bytes()));
                    buf.push(OsStr::from_bytes(env_path));
                    buf.push(OsStr::from_bytes(filename.as_bytes()));
                    if self.filesystem.open(&buf).is_ok() {
                        filename = CString::new(buf.into_os_string().as_bytes())?;
                        break;
                    }
                }
            }
        }

        Container::exec(
            self.filesystem,
            self.storage,
            filename,
            self.working_dir,
            argv,
            self.env,
        )
    }

    /// Append arguments to the container's command line
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self = self.arg(arg);
        }
        self
    }

    /// Append one argument to the container's command line
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        match CString::new(arg.as_ref().as_bytes()) {
            Err(e) => self.arg_error = Err(e),
            Ok(arg) => {
                if self.cmd_override.is_none() {
                    self.cmd_override = Some(Vec::new());
                }
                self.cmd_override.as_mut().unwrap().push(arg)
            }
        }
        self
    }

    /// Override the current directory the entrypoint will run in
    pub fn current_dir<P: AsRef<OsStr>>(mut self, dir: P) -> Self {
        match CString::new(dir.as_ref().as_bytes()) {
            Err(e) => self.arg_error = Err(e),
            Ok(arg) => self.working_dir = arg,
        }
        self
    }

    /// Override the container's entrypoint
    ///
    /// The entrypoint, if present, is prepended to the "args" to form
    /// the container's full command line.
    pub fn entrypoint<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut collected = Vec::new();
        for arg in args {
            match CString::new(arg.as_ref().as_bytes()) {
                Err(e) => {
                    self.arg_error = Err(e);
                    return self;
                }
                Ok(arg) => collected.push(arg),
            }
        }
        self.entrypoint = collected;
        self
    }

    /// Add or replace one environment variable
    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        if let Err(err) = env::set(
            &mut self.env,
            key.as_ref().as_bytes(),
            Some(val.as_ref().as_bytes()),
        ) {
            self.arg_error = Err(err)
        }
        self
    }

    /// Add or replace many environment variables
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (ref key, ref val) in vars {
            self = self.env(key, val);
        }
        self
    }

    /// Remove one environment variable entirely, leaving it unset
    pub fn env_remove<K: AsRef<OsStr>>(mut self, key: K) -> Self {
        env::remove(&mut self.env, key.as_ref().to_os_string().as_bytes());
        self
    }

    /// Clear all environment variables, including those from the image
    /// configuration
    pub fn env_clear(mut self) -> Self {
        self.env.clear();
        self
    }
}

mod env {
    use std::ffi::{CStr, CString, NulError};

    pub fn split<'a>(env: &'a CStr) -> (&'a [u8], Option<&'a CStr>) {
        let env = env.to_bytes_with_nul();
        let mut iter = env.splitn(2, |c| *c == b'=');
        let key = iter.next().unwrap();
        let value = iter.next().map(|b| CStr::from_bytes_with_nul(b).unwrap());
        (key, value)
    }

    pub fn join(key: &[u8], value: Option<&[u8]>) -> Result<CString, NulError> {
        let mut buf = Vec::new();
        buf.extend(key);
        if let Some(value) = value {
            buf.push(b'=');
            buf.extend(value);
        }
        CString::new(buf)
    }

    pub fn get<'a>(env: &'a Vec<CString>, key: &[u8]) -> Option<Option<&'a CStr>> {
        for item in env {
            let (item_key, item_value) = split(item);
            if item_key == key {
                return Some(item_value);
            }
        }
        None
    }

    pub fn remove(env: &mut Vec<CString>, key: &[u8]) -> Option<CString> {
        for (i, item) in env.iter().enumerate() {
            let (item_key, _) = split(item);
            if item_key == key {
                return Some(env.remove(i));
            }
        }
        None
    }

    pub fn set(env: &mut Vec<CString>, key: &[u8], value: Option<&[u8]>) -> Result<(), NulError> {
        let joined = join(key, value)?;
        for item in env.iter_mut() {
            let (item_key, _) = split(item);
            if item_key == key {
                *item = joined;
                return Ok(());
            }
        }
        Ok(env.push(joined))
    }
}
