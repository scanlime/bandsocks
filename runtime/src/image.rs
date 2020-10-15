use crate::{filesystem::vfs::Filesystem, manifest::RuntimeConfig};

#[derive(Debug)]
pub struct Image {
    pub digest: String,
    pub config: RuntimeConfig,
    pub filesystem: Filesystem,
}
