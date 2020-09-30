// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::mmap::MapRef;
use crate::errors::VFSError;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

type INodeNum = usize;

#[derive(Debug, Clone)]
pub struct Filesystem {
    inodes: Vec<Option<Arc<INode>>>,
    root: INodeNum,
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

    fn put_normal_file(&mut self, num: INodeNum, data: MapRef) {
        self.put_inode(num, INode {
            stat: Stat{
                mode: 0o644,
                ..Default::default()
            },
            data: Node::NormalFile(data)
        });
    }
    
    fn modify_directory(&mut self, parent: INodeNum, child_name: &OsStr, child_value: INodeNum) -> Result<(), VFSError> {
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
        self.modify_directory(parent, name, num)?;
        self.put_directory(num);
        self.modify_directory(num, &OsString::from(".."), parent)?;
        Ok(num)
    }

    fn alloc_file(&mut self, parent: INodeNum, name: &OsStr, data: MapRef) -> Result<INodeNum, VFSError> {
        let num = self.alloc_inode_number();
        self.modify_directory(parent, name, num)?;
        self.put_normal_file(num, data);
        Ok(num)
    }

    pub fn write_normal_file(&mut self, path: &Path, data: MapRef) -> Result<(), VFSError> {
        let mut dir = self.workdir;
        if let Some(parent) = path.parent() {
            dir = self.fs.resolve_path(dir, parent)?;
        }
        match path.file_name() {
            None => Err(VFSError::NotFound)?,
            Some(name) => {
                self.alloc_file(dir, name, data)?;
                Ok(())
            }
        }
    }
    
    pub fn mkdirp(&mut self, path: &Path) -> Result<(), VFSError> {
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
        Ok(())
    }

}

#[derive(Debug, Clone)]
struct INode {
    stat: Stat,
    data: Node,
}

#[derive(Debug, Clone, Default)]
struct Stat {
    mode: u64,
    uid: u64,
    gid: u64,
    mtime: u64,
}    

#[derive(Debug, Clone)]
enum Node {
    Directory(BTreeMap<OsString, INodeNum>),
    NormalFile(MapRef),
    SymbolicLink(PathBuf),
}
