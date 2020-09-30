// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::mmap::MapRef;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::path::PathBuf;

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
                content: Node::Directory( BTreeMap::new() )
            }))]
        }
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
    Directory(BTreeMap<PathBuf, u64>),
    NormalFile(MapRef),
    SymbolicLink(PathBuf),
}
