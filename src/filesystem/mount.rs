use crate::{errors::VFSError, filesystem::vfs::Filesystem};
use std::path::Path;

/// A trait for the ability to mount into a container's filesystem
pub trait Mount {
    fn mount(&self, fs: &mut Filesystem, path: &Path) -> Result<(), VFSError>;
}
