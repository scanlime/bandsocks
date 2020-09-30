// This code may not be used for any purpose. Be gay, do crime.

use crate::manifest::RuntimeConfig;
use crate::filesystem::vfs::Filesystem;

#[derive(Debug)]
pub struct Image {
    pub digest: String,
    pub config: RuntimeConfig,
    pub filesystem: Filesystem,
}
