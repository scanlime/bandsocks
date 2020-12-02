use crate::{
    errors::VFSError,
    filesystem::{
        mount::Mount,
        vfs::{Filesystem, Stat},
    },
};
use std::{
    fmt, io,
    os::unix::{
        io::{AsRawFd, RawFd},
        net::UnixStream,
    },
    path::Path,
    sync::Arc,
};

/// A single UnixStream which has been shared with a container
#[derive(Clone)]
pub struct SharedStream {
    inner: Arc<UnixStream>,
}

impl SharedStream {
    pub fn pair() -> io::Result<(UnixStream, SharedStream)> {
        let (local, remote) = UnixStream::pair()?;
        Ok((local, SharedStream::from_std(remote)))
    }

    pub fn from_std(stream: UnixStream) -> SharedStream {
        SharedStream {
            inner: Arc::new(stream),
        }
    }

    pub(crate) fn vfile_open(&self) -> Result<Arc<dyn AsRawFd + Sync + Send>, VFSError> {
        Ok(self.inner.clone())
    }
}

impl fmt::Debug for SharedStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SharedStream")
    }
}

impl AsRawFd for SharedStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl Mount for SharedStream {
    fn mount(&self, fs: &mut Filesystem, path: &Path) -> Result<(), VFSError> {
        let mut writer = fs.writer();
        let stat: Stat = Default::default();
        writer.write_shared_stream(path, stat, self.clone())
    }
}
