// This code may not be used for any purpose. Be gay, do crime.

use crate::errors::ImageError;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use memmap::Mmap;

#[derive(Debug, Clone)]
pub struct Filesystem {
    inodes: Vec<Option<Arc<INode>>>,
}

impl Filesystem {
    pub fn new() -> Self {
        Filesystem {
            inodes: vec![ Some( Arc::new( INode {
                mode: 0o755,
                user_id: 0,
                group_id: 0,
                content: Node::Directory( Directory::new() )
            }))]
        }
    }

    pub fn add_tar_overlay(&mut self, archive: &Arc<Mmap>) -> Result<(), ImageError> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct INode {
    mode: u64,
    user_id: u64,
    group_id: u64,
    content: Node,
}

#[derive(Debug, Clone)]
enum Node {
    Directory(Directory),
    NormalFile(NormalFile),
    SymbolicLink(SymbolicLink),
}

#[derive(Debug, Clone)]
struct Directory {
    contents: BTreeMap<PathBuf, u64>,
}

impl Directory {
    fn new() -> Self {
        Directory {
            contents: BTreeMap::new(),
        }
    }
}
            
#[derive(Debug, Clone)]
struct NormalFile {
    source: Arc<Mmap>,
    offset: u64,
    filesize: u64,
}

#[derive(Debug, Clone)]
struct SymbolicLink {
    target: PathBuf,
}
