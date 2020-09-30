// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::vfs::Filesystem;
use crate::errors::ImageError;
use std::sync::Arc;
use memmap::Mmap;

pub fn extract_metadata(fs: &mut Filesystem, archive: &Arc<Mmap>) -> Result<(), ImageError> {
    Ok(())
}
