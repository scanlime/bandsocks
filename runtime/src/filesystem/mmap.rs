// This code may not be used for any purpose. Be gay, do crime.

use std::sync::Arc;
use std::ops::Deref;
use memmap::Mmap;

#[derive(Debug, Clone)]
pub struct MapRef {
    source: Arc<Mmap>,
    offset: usize,
    filesize: usize,
}

impl MapRef {
    pub fn new(source: &Arc<Mmap>, offset: usize, filesize: usize) -> Self {
        MapRef {
            source: source.clone(),
            offset,
            filesize
        }
    }
}

impl Deref for MapRef {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let start = self.offset;
        let end = start + self.filesize;
        &self.source[start..end]
    }
}

impl AsRef<[u8]> for MapRef {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}
