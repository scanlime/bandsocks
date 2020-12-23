use crate::{
    container::{Container, ExitStatus, Output},
    errors::{ImageError, RuntimeError, VFSError},
    filesystem::{mount::Mount, socket::SharedStream, storage::FileStorage, vfs::Filesystem},
    manifest::ImageConfig,
    sand,
    sand::protocol::{FollowLinks, TracerSettings},
};
use std::{
    ffi::{CString, NulError, OsStr},
    os::unix::{ffi::OsStrExt, net::UnixStream},
    path::{Path, PathBuf},
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
    mount_error: Result<(), VFSError>,
    stdio: [Option<SharedStream>; 3],
    tracer_settings: TracerSettings,
}

impl ContainerBuilder {
    pub(crate) fn new(
        config: &ImageConfig,
        filesystem: Filesystem,
        storage: FileStorage,
    ) -> Result<Self, ImageError> {
        Ok(ContainerBuilder {
            filesystem,
            storage,
            tracer_settings: TracerSettings {
                max_log_level: sand::max_log_level(),
                instruction_trace: false,
            },
            arg_error: Ok(()),
            mount_error: Ok(()),
            stdio: [None, None, None],
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

    /// Start a new [Container] using these settings, and wait for it to exit,
    /// returning its exit status
    ///
    /// This is equivalent to calling spawn() first and then
    /// [Container::wait()].
    pub async fn run(self) -> Result<ExitStatus, RuntimeError> {
        self.spawn()?.wait().await
    }

    /// Start a new [Container] using these settings, and wait for it to exit,
    /// returning its output
    ///
    /// This is equivalent to calling spawn() first and then
    /// [Container::output()].
    pub async fn output(self) -> Result<Output, RuntimeError> {
        self.spawn()?.output().await
    }

    /// Start a new [Container] using these settings, and wait for it to exit
    /// with its stdio streams connected.
    ///
    /// This is equivalent to calling spawn() first and then
    /// [Container::interact()].
    pub async fn interact(self) -> Result<ExitStatus, RuntimeError> {
        self.spawn()?.interact().await
    }

    /// Start a new [Container] using the settings in this builder
    pub fn spawn(mut self) -> Result<Container, RuntimeError> {
        self.arg_error?;
        self.mount_error?;

        let mut local_stdio: [Option<UnixStream>; 3] = [None, None, None];
        for fd in 0..3 {
            let remote_stream = match self.stdio[fd].take() {
                Some(stream) => stream,
                None => {
                    let (local, remote) = SharedStream::pair()?;
                    local_stdio[fd] = Some(local);
                    remote
                }
            };
            remote_stream.mount(
                &mut self.filesystem,
                &Path::new(&format!("/proc/1/fd/{}", fd)),
            )?;
        }

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
                    if self
                        .filesystem
                        .lookup(&Filesystem::root(), &buf, FollowLinks::Follow)
                        .is_ok()
                    {
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
            local_stdio,
            self.tracer_settings,
        )
    }

    /// Mount an overlay on the container's filesystem
    ///
    /// [Mount] objects can write to the container's filesystem metadata at
    /// startup, leaving static files and/or live communications channels.
    pub fn mount<P, T>(mut self, path: P, mount: &T) -> Self
    where
        P: AsRef<Path>,
        T: Mount,
    {
        self.mount_error = self
            .mount_error
            .and(mount.mount(&mut self.filesystem, path.as_ref()));
        self
    }

    /// Attach stdin to a specific shared stream
    pub fn stdin(mut self, stream: SharedStream) -> Self {
        self.stdio[0] = Some(stream);
        self
    }

    /// Attach stdout to a specific shared stream
    pub fn stdout(mut self, stream: SharedStream) -> Self {
        self.stdio[1] = Some(stream);
        self
    }

    /// Attach stderr to a specific shared stream
    pub fn stderr(mut self, stream: SharedStream) -> Self {
        self.stdio[2] = Some(stream);
        self
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
    pub fn arg<S>(mut self, arg: S) -> Self
    where
        S: AsRef<OsStr>,
    {
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

    /// Override the working directory the entrypoint will start in
    pub fn working_dir<P>(mut self, dir: P) -> Self
    where
        P: AsRef<Path>,
    {
        match CString::new(dir.as_ref().as_os_str().as_bytes()) {
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

    /// Run the container in single-step mode
    ///
    /// This is extremely verbose, and intended only for debugging or reporting
    /// internal problems with the sandbox runtime.
    pub fn instruction_trace(mut self) -> Self {
        self.tracer_settings.instruction_trace = true;
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
