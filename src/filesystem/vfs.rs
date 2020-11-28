use crate::{
    errors::VFSError,
    filesystem::storage::{FileStorage, StorageKey},
};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::File,
    future::Future,
    os::unix::{io::AsRawFd, net::UnixStream},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

type INodeNum = usize;

#[derive(Debug, Clone, Default)]
pub struct Stat {
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
    pub mtime: u64,
    pub nlink: u64,
    pub size: u64,
}

#[derive(Clone)]
pub struct Filesystem {
    inodes: Vec<Option<Arc<INode>>>,
    root: INodeNum,
}

#[derive(Debug, Clone)]
pub struct VFile {
    inode: INodeNum,
}

pub struct VFSWriter<'f> {
    fs: &'f mut Filesystem,
    workdir: INodeNum,
}

#[derive(Clone)]
struct INode {
    stat: Stat,
    data: Node,
}

#[derive(Debug, Clone)]
struct DirEntryRef {
    parent: INodeNum,
    child: INodeNum,
}

#[derive(Clone)]
enum Node {
    NormalDirectory(BTreeMap<OsString, INodeNum>),
    FileStorage(StorageKey),
    FileFactory(AsyncFactory<File>),
    UnixStreamFactory(AsyncFactory<UnixStream>),
    EmptyFile,
    SymbolicLink(PathBuf),
    Char(u32, u32),
    Block(u32, u32),
    Fifo,
}

pub type AsyncFactory<T> =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<T, VFSError>> + Sync + Send>> + Sync + Send>;

#[derive(Debug)]
struct Limits {
    path_segment: usize,
    symbolic_link: usize,
}

impl Limits {
    fn reset() -> Self {
        Limits {
            path_segment: 1000,
            symbolic_link: 50,
        }
    }

    fn take_path_segment(&mut self) -> Result<(), VFSError> {
        if self.path_segment > 0 {
            self.path_segment -= 1;
            Ok(())
        } else {
            Err(VFSError::PathSegmentLimitExceeded)
        }
    }

    fn take_symbolic_link(&mut self) -> Result<(), VFSError> {
        if self.symbolic_link > 0 {
            self.symbolic_link -= 1;
            Ok(())
        } else {
            Err(VFSError::SymbolicLinkLimitExceeded)
        }
    }
}

impl<'s> Filesystem {
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

    pub fn writer<'f>(&'f mut self) -> VFSWriter<'f> {
        let workdir = self.root;
        VFSWriter { workdir, fs: self }
    }

    fn get_inode(&self, num: INodeNum) -> Result<&INode, VFSError> {
        match self.inodes.get(num) {
            None => Err(VFSError::UnallocNode),
            Some(slice) => match slice {
                None => Err(VFSError::UnallocNode),
                Some(node) => Ok(node),
            },
        }
    }

    fn resolve_symlinks(
        &self,
        mut limits: &mut Limits,
        mut entry: DirEntryRef,
    ) -> Result<DirEntryRef, VFSError> {
        while let Node::SymbolicLink(path) = &self.get_inode(entry.child)?.data {
            log::trace!("following symlink, {:?} -> {:?}", entry, path);
            limits.take_symbolic_link()?;
            entry = self.resolve_path(&mut limits, entry.parent, path)?;
        }
        Ok(entry)
    }

    fn resolve_path_segment(
        &self,
        limits: &mut Limits,
        parent: INodeNum,
        part: &OsStr,
    ) -> Result<DirEntryRef, VFSError> {
        limits.take_path_segment()?;
        if part == "/" {
            Ok(DirEntryRef {
                parent: self.root,
                child: self.root,
            })
        } else {
            match &self.get_inode(parent)?.data {
                Node::NormalDirectory(map) => match map.get(part) {
                    None => Err(VFSError::NotFound),
                    Some(child) => {
                        let entry = DirEntryRef {
                            parent,
                            child: *child,
                        };
                        Ok(entry)
                    }
                },
                other => Err(VFSError::DirectoryExpected),
            }
        }
    }

    fn resolve_path(
        &self,
        mut limits: &mut Limits,
        parent: INodeNum,
        path: &Path,
    ) -> Result<DirEntryRef, VFSError> {
        // resolve symlinks in-between steps but not before the first step
        // (workdir must be a directory and not a symlink) or after the
        // last step (the result itself might be a link).

        let mut iter = path.iter();
        let result = if let Some(part) = iter.next() {
            let mut entry = self.resolve_path_segment(&mut limits, parent, part)?;

            while let Some(part) = iter.next() {
                entry = self.resolve_symlinks(&mut limits, entry)?;
                entry = self.resolve_path_segment(&mut limits, entry.child, part)?;
            }

            Ok(entry)
        } else {
            Ok(DirEntryRef {
                parent,
                child: parent,
            })
        };

        log::trace!("resolved path {:?} in {} -> {:?}", path, parent, result);
        result
    }

    pub fn open_root(&self) -> VFile {
        self.open(Path::new("/")).unwrap()
    }

    pub fn open(&self, path: &Path) -> Result<VFile, VFSError> {
        self.open_at(None, path)
    }

    pub fn open_at(&self, at_dir: Option<&VFile>, path: &Path) -> Result<VFile, VFSError> {
        log::debug!("open({:?}, {:?})", at_dir, path);
        let mut limits = Limits::reset();
        let entry = self.resolve_path(&mut limits, self.root, path)?;
        let entry = self.resolve_symlinks(&mut limits, entry)?;
        Ok(VFile { inode: entry.child })
    }

    pub fn vfile_stat<'a>(&'a self, f: &VFile) -> Result<&'a Stat, VFSError> {
        match &self.inodes[f.inode] {
            None => Err(VFSError::NotFound),
            Some(node) => Ok(&node.stat),
        }
    }

    pub async fn vfile_storage(
        &self,
        storage: &FileStorage,
        f: &VFile,
    ) -> Result<Box<dyn AsRawFd + Sync + Send>, VFSError> {
        match &self.inodes[f.inode] {
            None => Err(VFSError::NotFound),
            Some(node) => match &node.data {
                Node::EmptyFile => Ok(Box::new(
                    File::open("/dev/null").map_err(|_| VFSError::ImageStorageError)?,
                )),
                Node::FileStorage(key) => Ok(Box::new(
                    storage
                        .open_part(key)
                        .await
                        .ok()
                        .flatten()
                        .ok_or(VFSError::ImageStorageError)?,
                )),
                Node::FileFactory(factory) => Ok(Box::new(factory().await?)),
                Node::UnixStreamFactory(factory) => Ok(Box::new(factory().await?)),
                _ => Err(VFSError::FileExpected),
            },
        }
    }

    pub fn is_directory(&self, f: &VFile) -> bool {
        match &self.inodes[f.inode] {
            None => false,
            Some(node) => match &node.data {
                Node::NormalDirectory(_) => true,
                _ => false,
            },
        }
    }
}

impl<'f> VFSWriter<'f> {
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
            },
        }
    }

    fn put_inode(&mut self, num: INodeNum, inode: INode) {
        assert!(self.fs.inodes[num as usize].is_none());
        self.fs.inodes[num].replace(Arc::new(inode));
    }

    fn put_directory(&mut self, num: INodeNum) {
        let mut map = BTreeMap::new();
        map.insert(OsString::from("."), num);

        self.put_inode(
            num,
            INode {
                stat: Stat {
                    mode: 0o755,
                    nlink: 1,
                    ..Default::default()
                },
                data: Node::NormalDirectory(map),
            },
        );
    }

    fn inode_incref(&mut self, num: INodeNum) -> Result<(), VFSError> {
        let mut stat = &mut self.get_inode_mut(num)?.stat;
        match stat.nlink.checked_add(1) {
            None => Err(VFSError::INodeRefCountError),
            Some(count) => {
                stat.nlink = count;
                Ok(())
            }
        }
    }

    fn inode_decref(&mut self, num: INodeNum) -> Result<(), VFSError> {
        let mut stat = &mut self.get_inode_mut(num)?.stat;
        match stat.nlink.checked_sub(1) {
            None => Err(VFSError::INodeRefCountError),
            Some(count) => {
                stat.nlink = count;
                Ok(())
            }
        }
    }

    fn add_child_to_directory(
        &mut self,
        parent: INodeNum,
        child_name: &OsStr,
        child_value: INodeNum,
    ) -> Result<(), VFSError> {
        self.inode_incref(child_value)?;
        let previous = match &mut self.get_inode_mut(parent)?.data {
            Node::NormalDirectory(map) => map.insert(child_name.to_os_string(), child_value),
            other => Err(VFSError::DirectoryExpected)?,
        };
        match previous {
            None => Ok(()),
            Some(prev_child) => self.inode_decref(prev_child),
        }
    }

    fn alloc_child_directory(
        &mut self,
        parent: INodeNum,
        name: &OsStr,
    ) -> Result<INodeNum, VFSError> {
        let num = self.alloc_inode_number();
        self.put_directory(num);
        self.add_child_to_directory(parent, name, num)?;
        self.add_child_to_directory(num, &OsString::from(".."), parent)?;
        Ok(num)
    }

    fn resolve_or_create_parent<'b>(
        &mut self,
        mut limits: &mut Limits,
        path: &'b Path,
    ) -> Result<(INodeNum, &'b OsStr), VFSError> {
        let dir = if let Some(parent) = path.parent() {
            let entry = self.resolve_or_create_path(&mut limits, self.workdir, parent)?;
            let entry = self.fs.resolve_symlinks(&mut limits, entry)?;
            entry.child
        } else {
            self.workdir
        };
        match path.file_name() {
            None => Err(VFSError::NotFound),
            Some(name) => Ok((dir, name)),
        }
    }

    pub fn write_directory_metadata(&mut self, path: &Path, stat: Stat) -> Result<(), VFSError> {
        let mut limits = Limits::reset();
        let entry = self.resolve_or_create_path(&mut limits, self.workdir, path)?;
        let entry = self.fs.resolve_symlinks(&mut limits, entry)?;
        let inode = self.get_inode_mut(entry.child)?;
        if let Node::NormalDirectory(_) = inode.data {
            inode.stat = stat;
            Ok(())
        } else {
            Err(VFSError::DirectoryExpected)
        }
    }

    fn write_node_file(&mut self, path: &Path, stat: Stat, data: Node) -> Result<(), VFSError> {
        let mut limits = Limits::reset();
        let (dir, name) = self.resolve_or_create_parent(&mut limits, path)?;
        let num = self.alloc_inode_number();
        self.put_inode(num, INode { stat, data });
        self.add_child_to_directory(dir, name, num)?;
        Ok(())
    }

    pub fn write_storage_file(
        &mut self,
        path: &Path,
        stat: Stat,
        data: Option<StorageKey>,
    ) -> Result<(), VFSError> {
        self.write_node_file(
            path,
            stat,
            match data {
                Some(key) => Node::FileStorage(key),
                None => Node::EmptyFile,
            },
        )
    }

    pub fn write_file_factory(
        &mut self,
        path: &Path,
        stat: Stat,
        factory: AsyncFactory<File>,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::FileFactory(factory))
    }

    pub fn write_unix_stream_factory(
        &mut self,
        path: &Path,
        stat: Stat,
        factory: AsyncFactory<UnixStream>,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::UnixStreamFactory(factory))
    }

    pub fn write_symlink(
        &mut self,
        path: &Path,
        stat: Stat,
        link_to: &Path,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::SymbolicLink(link_to.to_path_buf()))
    }

    pub fn write_hardlink(&mut self, path: &Path, link_to: &Path) -> Result<(), VFSError> {
        let mut limits = Limits::reset();
        let link_to_node = self
            .fs
            .resolve_path(&mut limits, self.workdir, link_to)?
            .child;
        let (dir, name) = self.resolve_or_create_parent(&mut limits, path)?;
        self.add_child_to_directory(dir, name, link_to_node)?;
        Ok(())
    }

    pub fn write_fifo(&mut self, path: &Path, stat: Stat) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::Fifo)
    }

    pub fn write_char_device(
        &mut self,
        path: &Path,
        stat: Stat,
        major: u32,
        minor: u32,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::Char(major, minor))
    }

    pub fn write_block_device(
        &mut self,
        path: &Path,
        stat: Stat,
        major: u32,
        minor: u32,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::Block(major, minor))
    }

    fn resolve_or_create_path_segment(
        &mut self,
        mut limits: &mut Limits,
        parent: INodeNum,
        part: &OsStr,
    ) -> Result<DirEntryRef, VFSError> {
        let result = self.fs.resolve_path_segment(&mut limits, parent, part);
        match result {
            Ok(entry) => Ok(entry),
            Err(VFSError::NotFound) => {
                let child = self.alloc_child_directory(parent, part)?;
                Ok(DirEntryRef { parent, child })
            }
            Err(other) => Err(other),
        }
    }

    fn resolve_or_create_path(
        &mut self,
        mut limits: &mut Limits,
        parent: INodeNum,
        path: &Path,
    ) -> Result<DirEntryRef, VFSError> {
        let mut iter = path.iter();
        if let Some(part) = iter.next() {
            let mut entry = self.resolve_or_create_path_segment(&mut limits, parent, part)?;
            while let Some(part) = iter.next() {
                entry = self.fs.resolve_symlinks(&mut limits, entry)?;
                entry = self.resolve_or_create_path_segment(&mut limits, entry.child, part)?;
            }
            Ok(entry)
        } else {
            Ok(DirEntryRef {
                parent,
                child: parent,
            })
        }
    }
}
