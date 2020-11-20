//! Optional lower-level interface to stored container images

use crate::{
    filesystem::{storage::FileStorage, vfs::Filesystem},
    manifest::RuntimeConfig,
};

/// Internal representation of a container image that's loaded and ready to
/// launch
#[derive(Debug)]
pub struct Image {
    pub(crate) digest: String,
    pub(crate) config: RuntimeConfig,
    pub(crate) filesystem: Filesystem,
    pub(crate) storage: FileStorage,
}

impl Image {
    /// Get the digest uniquely identifying this image content
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
