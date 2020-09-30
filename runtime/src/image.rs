// This code may not be used for any purpose. Be gay, do crime.

use crate::manifest::RuntimeConfig;
use std::sync::Arc;
use memmap::Mmap;

#[derive(Debug, Clone)]
pub struct Image {
    pub digest: String,
    pub config: RuntimeConfig,
    pub content: Vec<Arc<Mmap>>,
}
