use crate::manifest::RuntimeConfig;
use crate::filesystem::vfs::Filesystem;

#[derive(Debug)]
pub struct Image {
    pub digest: String,
    pub config: RuntimeConfig,
    pub filesystem: Filesystem,
}
