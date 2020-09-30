// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::mmap::MapRef;
use crate::errors::VFSError;
use std::fmt;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

type INodeNum = usize;

#[derive(Clone, Default)]
pub struct Stat {
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
    pub mtime: u64,
}

impl fmt::Debug for Stat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("{:o} {}:{} @{}", self.mode, self.uid, self.gid, self.mtime))
    }
}

#[derive(Clone)]
pub struct Filesystem {
    inodes: Vec<Option<Arc<INode>>>,
    root: INodeNum,
}

impl fmt::Debug for Filesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut stack = vec![( PathBuf::new(), self.root )];        
        let mut memo = HashSet::new();
        while let Some((path, dir)) = stack.pop() {
            memo.insert(dir);
            match self.get_inode(dir) {
                Ok(inode) => match &inode.data {
                    Node::Directory(map) => {
                        for (name, child) in map.iter() {
                            let child_path = path.join(name);
                            match self.get_inode(*child) {
                                Ok(child_node) => {
                                    match &child_node.data {
                                        Node::Directory(_) => {
                                            if !memo.contains(child) {
                                                stack.push((child_path, *child));
                                            }
                                        },
                                        Node::NormalFile(file) => {
                                            f.write_fmt(format_args!("{:30} {:8}  /{}\n",
                                                                     format!("{:?}", child_node.stat),
                                                                     file.len(),
                                                                     child_path.to_string_lossy()))?;
                                        },
                                        other => {
                                            f.write_fmt(format_args!("{:30} {:?}  /{}\n",
                                                                     format!("{:?}", child_node.stat),
                                                                     other,
                                                                     child_path.to_string_lossy()))?;
                                        }
                                    }
                                },
                                other => {
                                    f.write_fmt(format_args!("<<ERROR>>, failed to read child inode, {:?}", other))?;
                                }
                            }
                        }
                    }
                    other => {
                        f.write_fmt(format_args!("<<ERROR>>, expected directory at inode {}, found: {:?}", dir, other))?;
                    }
                },
                other => {
                    f.write_fmt(format_args!("<<ERROR>>, failed to read directory inode {}, {:?}", dir, other))?;
                }
            }
        }
        Ok(())
    }
}

impl Filesystem {
    pub fn new() -> Self {
        let root = 0;
        let mut fs = Filesystem {
            root,
            inodes: vec![None],
        };
        fs.writer().put_directory(root);
        assert_eq!(root, fs.root);
        fs
    }

    pub fn writer<'a>(&'a mut self) -> VFSWriter<'a> {
        let workdir = self.root;
        VFSWriter {
            fs: self,
            workdir
        }
    }            

    fn get_inode(&self, num: INodeNum) -> Result<&INode, VFSError> {
        match self.inodes.get(num) {
            None => Err(VFSError::UnallocNode),
            Some(slice) => match slice {
                None => Err(VFSError::UnallocNode),
                Some(node) => Ok(node),
            }
        }
    }

    fn resolve_path_segment(&self, workdir: INodeNum, part: &OsStr) -> Result<INodeNum, VFSError> {
        match &self.get_inode(workdir)?.data {
            Node::Directory(map) => {
                match map.get(part) {
                    None => Err(VFSError::NotFound)?,
                    Some(child_node) => Ok(*child_node),
                }
            },
            _ => Err(VFSError::DirectoryExpected)?,
        }
    }
                
    fn resolve_path(&self, workdir: INodeNum, path: &Path) -> Result<INodeNum, VFSError> {
        let mut node = workdir;
        for part in path.iter() {
            node = self.resolve_path_segment(node, part)?;
        }
        Ok(node)
    }
}

pub struct VFSWriter<'a> {
    fs: &'a mut Filesystem,
    workdir: INodeNum,
}

impl<'a> VFSWriter<'a> {
    fn alloc_inode_number(&mut self) -> INodeNum {
        let num = self.fs.inodes.len() as INodeNum;
        self.fs.inodes.push(None);
        num
    }

    fn get_inode_mut(&mut self, num: INodeNum) -> Result<&mut INode, VFSError> {
        match self.fs.inodes.get_mut(num) {
            None => Err(VFSError::UnallocNode),
            Some(slice) => match slice {
                None => Err(VFSError::UnallocNode),
                Some(node) => Ok(Arc::make_mut(node)),
            }
        }
    }

    fn put_inode(&mut self, num: INodeNum, inode: INode) {
        assert!(self.fs.inodes[num as usize].is_none());
        self.fs.inodes[num].replace(Arc::new(inode));
    }
    
    fn put_directory(&mut self, num: INodeNum) {
        let mut map = BTreeMap::new();
        map.insert(OsString::from("."), num);

        self.put_inode(num, INode {
            stat: Stat{
                mode: 0o755,
                ..Default::default()
            },
            data: Node::Directory(map)
        });
    }

    fn add_child_to_directory(&mut self, parent: INodeNum, child_name: &OsStr, child_value: INodeNum) -> Result<(), VFSError> {
        match &mut self.get_inode_mut(parent)?.data {
            Node::Directory(map) => {
                map.insert(child_name.to_os_string(), child_value);
                Ok(())
            }
            _ => Err(VFSError::DirectoryExpected)?,
        }
    }
    
    fn alloc_child_directory(&mut self, parent: INodeNum, name: &OsStr) -> Result<INodeNum, VFSError> {
        let num = self.alloc_inode_number();
        self.add_child_to_directory(parent, name, num)?;
        self.put_directory(num);
        self.add_child_to_directory(num, &OsString::from(".."), parent)?;
        Ok(num)
    }
    
    pub fn write_directory_metadata(&mut self, path: &Path, stat: Stat) -> Result<(), VFSError> {
        let dir = self.resolve_or_create_path(path)?;
        let inode = self.get_inode_mut(dir)?;
        if let Node::Directory(_) = inode.data {
            std::mem::replace(&mut inode.stat, stat);
            Ok(())
        } else {
            Err(VFSError::DirectoryExpected)
        }
    }
    
    pub fn write_file_mapping(&mut self, path: &Path, data: MapRef, stat: Stat) -> Result<(), VFSError> {
        let dir = if let Some(parent) = path.parent() {
            self.resolve_or_create_path(parent)?
        } else {
            self.workdir
        };
        match path.file_name() {
            None => Err(VFSError::NotFound)?,
            Some(name) => {
                let num = self.alloc_inode_number();
                self.add_child_to_directory(dir, name, num)?;
                self.put_inode(num, INode {
                    stat,
                    data: Node::NormalFile(data)
                });
                Ok(())
            }
        }
    }
    
    fn resolve_or_create_path(&mut self, path: &Path) -> Result<INodeNum, VFSError> {
        let mut dir = self.workdir;
        for part in path.iter() {
            match self.fs.resolve_path_segment(dir, part) {
                Ok(child) => {
                    dir = child;
                },
                Err(VFSError::NotFound) => {
                    dir = self.alloc_child_directory(dir, part)?;
                },
                Err(other) => Err(other)?,
            }
        }
        Ok(dir)
    }

}

#[derive(Debug, Clone)]
struct INode {
    stat: Stat,
    data: Node,
}

#[derive(Debug, Clone)]
enum Node {
    Directory(BTreeMap<OsString, INodeNum>),
    NormalFile(MapRef),
    SymbolicLink(PathBuf),
}
