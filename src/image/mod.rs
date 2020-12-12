//! Container images and image identity

#[cfg(test)] mod tests;

mod digest;
mod name;
mod registry;
mod repository;
mod tag;
mod version;

pub use digest::ContentDigest;
pub use name::ImageName;
pub use registry::Registry;
pub use repository::{Repository, RepositoryIter};
pub use tag::Tag;
pub use version::ImageVersion;

use crate::{
    filesystem::{storage::FileStorage, vfs::Filesystem},
    manifest::RuntimeConfig,
};
use std::fmt;

/// Loaded data for a container image
///
/// This is the actual configuration and filesystem data associated with a
/// container image. It is immutable, and multiple running containers can use
/// one image.
///
/// The virtual filesystem stores all metadata in memory, but file contents are
/// referenced as needed from the configured disk cache.
pub struct Image {
    pub(crate) name: ImageName,
    pub(crate) config: RuntimeConfig,
    pub(crate) filesystem: Filesystem,
    pub(crate) storage: FileStorage,
}

impl Image {
    /// Get the digest identifying this image's content and configuration
    pub fn content_digest(&self) -> ContentDigest {
        self.name()
            .content_digest()
            .expect("loaded images must always have a digest")
    }

    /// Get the name of this image, including its content digest
    pub fn name(&self) -> &ImageName {
        &self.name
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({})", self.name)
    }
}
