use memmap::{Mmap, MmapOptions};
use std::{
    fs::File,
    io,
    ops::{Deref, Range},
    os::unix::{io::AsRawFd, prelude::RawFd},
    path::Path,
    sync::{Arc, Weak},
};

#[derive(Debug)]
struct Source {
    file: File,
    map: Mmap,
}

#[derive(Debug, Clone)]
pub struct WeakMapRef {
    source: Weak<Source>,
    offset: usize,
    filesize: usize,
}

impl WeakMapRef {
    pub fn upgrade(&self) -> Option<MapRef> {
        self.source.upgrade().map(|source| MapRef {
            source,
            offset: self.offset,
            filesize: self.filesize,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MapRef {
    source: Arc<Source>,
    offset: usize,
    filesize: usize,
}

impl MapRef {
    pub fn open(path: &Path) -> Result<MapRef, io::Error> {
        let file = File::open(path)?;
        let map = unsafe { MmapOptions::new().map(&file) }?;
        let filesize = map.len();
        Ok(MapRef {
            source: Arc::new(Source { file, map }),
            offset: 0,
            filesize,
        })
    }

    pub fn downgrade(&self) -> WeakMapRef {
        WeakMapRef {
            source: Arc::downgrade(&self.source),
            offset: self.offset,
            filesize: self.filesize,
        }
    }

    pub fn range(&self, range: &Range<usize>) -> Result<MapRef, ()> {
        if range.end <= self.filesize {
            Ok(MapRef {
                source: self.source.clone(),
                offset: self.offset + range.start,
                filesize: range.end - range.start,
            })
        } else {
            Err(())
        }
    }

    pub fn len(&self) -> usize {
        self.filesize
    }

    pub fn source_fd(&self) -> RawFd {
        self.source.file.as_raw_fd()
    }

    pub fn source_offset(&self) -> usize {
        self.offset
    }
}

impl Deref for MapRef {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let start = self.offset;
        let end = start + self.filesize;
        &self.source.map[start..end]
    }
}

impl AsRef<[u8]> for MapRef {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}
