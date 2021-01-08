use crate::{
    errors::VFSError,
    filesystem::{
        socket::SharedStream,
        storage::{FileStorage, StorageKey},
    },
    sand::protocol::{abi, abi::DirentHeader, FileStat, FollowLinks, INodeNum, VFile},
};
use plain::Plain;
use std::{
    collections::BTreeMap,
    convert::TryInto,
    ffi::{CStr, CString, OsStr, OsString},
    fs::File,
    io::{BufWriter, Seek, SeekFrom, Write},
    os::unix::{ffi::OsStrExt, io::AsRawFd},
    path::Path,
    sync::Arc,
};

#[derive(Clone)]
pub struct Filesystem {
    inodes: Vec<Option<Arc<INode>>>,
}

pub struct VFSWriter<'f> {
    workdir: VFile,
    fs: &'f mut Filesystem,
}

#[derive(Clone)]
struct INode {
    stat: FileStat,
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
    SharedStream(SharedStream),
    EmptyFile,
    SymbolicLink(CString),
    Char(u32, u32),
    Block(u32, u32),
    Fifo,
}

#[repr(C)]
struct PlainDirentHeader(DirentHeader);

unsafe impl Plain for PlainDirentHeader {}

#[derive(Debug)]
struct Limits {
    path_segment: usize,
    symbolic_link: usize,
}

impl DirEntryRef {
    fn root() -> Self {
        DirEntryRef {
            parent: Filesystem::root().inode,
            child: Filesystem::root().inode,
        }
    }
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
        let mut fs = Filesystem { inodes: vec![None] };
        let root = Filesystem::root().inode;
        fs.writer().put_directory(root);
        fs
    }

    pub fn writer<'f>(&'f mut self) -> VFSWriter<'f> {
        let workdir = Filesystem::root();
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
        while let Node::SymbolicLink(cstr) = &self.get_inode(entry.child)?.data {
            log::trace!("following symlink, {:?} -> {:?}", entry, cstr);
            limits.take_symbolic_link()?;
            entry = self.resolve_path(
                &mut limits,
                entry.parent,
                Path::new(cstr.as_c_str().to_str()?),
            )?;
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
            Ok(DirEntryRef::root())
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
                _ => Err(VFSError::DirectoryExpected),
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

        result
    }

    pub fn root() -> VFile {
        VFile { inode: 0 }
    }

    pub fn lookup(
        &self,
        dir: &VFile,
        path: &Path,
        follow_links: &FollowLinks,
    ) -> Result<VFile, VFSError> {
        let mut limits = Limits::reset();
        let entry = self.resolve_path(&mut limits, dir.inode, path)?;
        let entry = match follow_links {
            FollowLinks::NoFollow => entry,
            FollowLinks::Follow => self.resolve_symlinks(&mut limits, entry)?,
        };
        log::debug!(
            "open({:?}, {:?}, {:?}) -> {:?}",
            dir,
            path,
            follow_links,
            entry
        );
        Ok(VFile { inode: entry.child })
    }

    pub fn stat(&self, f: &VFile) -> Result<&FileStat, VFSError> {
        let stat = &self.get_inode(f.inode)?.stat;
        log::debug!("stat({:?}) -> {:?}", f, stat);
        Ok(stat)
    }

    pub fn readlink(&self, f: &VFile) -> Result<&CStr, VFSError> {
        let cstr = match &self.get_inode(f.inode)?.data {
            Node::SymbolicLink(path) => path.as_c_str(),
            _ => return Err(VFSError::LinkExpected),
        };
        log::debug!("readlink({:?}) -> {:?}", f, cstr);
        Ok(cstr)
    }

    pub async fn open_storage(
        &self,
        storage: &FileStorage,
        f: &VFile,
    ) -> Result<Arc<dyn AsRawFd + Sync + Send>, VFSError> {
        let node = self.get_inode(f.inode)?;
        match &node.data {
            Node::EmptyFile => open_null(),
            Node::NormalDirectory(dir) => self.open_directory(dir),
            Node::SharedStream(stream) => stream.vfile_open(),
            Node::FileStorage(key) => open_storage_part(storage, key).await,
            _ => return Err(VFSError::FileExpected),
        }
    }

    pub fn is_directory(&self, f: &VFile) -> Result<bool, VFSError> {
        let node = self.get_inode(f.inode)?;
        match &node.data {
            Node::NormalDirectory(_) => Ok(true),
            _ => Ok(false),
        }
    }

    fn dir_entry_type(&self, inode: INodeNum) -> Result<u8, VFSError> {
        let stat = &self.get_inode(inode)?.stat;
        Ok(match stat.st_mode & abi::S_IFMT {
            abi::S_IFSOCK => abi::DT_SOCK,
            abi::S_IFLNK => abi::DT_LNK,
            abi::S_IFREG => abi::DT_REG,
            abi::S_IFBLK => abi::DT_BLK,
            abi::S_IFDIR => abi::DT_DIR,
            abi::S_IFCHR => abi::DT_CHR,
            abi::S_IFIFO => abi::DT_FIFO,
            _ => abi::DT_UNKNOWN,
        })
    }

    fn open_directory(
        &self,
        dir: &BTreeMap<OsString, INodeNum>,
    ) -> Result<Arc<dyn AsRawFd + Sync + Send>, VFSError> {
        let mut builder = DirectoryFileBuilder::new()?;
        for (name, inode) in dir.iter() {
            let file_type = self.dir_entry_type(*inode)?;
            builder.append(name.as_bytes(), *inode as u64, file_type)?;
        }
        builder.finish()
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
                stat: FileStat {
                    st_mode: 0o755 | abi::S_IFDIR,
                    st_nlink: 1,
                    ..Default::default()
                },
                data: Node::NormalDirectory(map),
            },
        );
    }

    fn inode_incref(&mut self, num: INodeNum) -> Result<(), VFSError> {
        let mut stat = &mut self.get_inode_mut(num)?.stat;
        match stat.st_nlink.checked_add(1) {
            None => Err(VFSError::INodeRefCountError),
            Some(count) => {
                stat.st_nlink = count;
                Ok(())
            }
        }
    }

    fn inode_decref(&mut self, num: INodeNum) -> Result<(), VFSError> {
        let mut stat = &mut self.get_inode_mut(num)?.stat;
        match stat.st_nlink.checked_sub(1) {
            None => Err(VFSError::INodeRefCountError),
            Some(count) => {
                stat.st_nlink = count;
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
            _ => Err(VFSError::DirectoryExpected)?,
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
            let entry = self.resolve_or_create_path(&mut limits, self.workdir.inode, parent)?;
            let entry = self.fs.resolve_symlinks(&mut limits, entry)?;
            entry.child
        } else {
            self.workdir.inode
        };
        match path.file_name() {
            None => Err(VFSError::NotFound),
            Some(name) => Ok((dir, name)),
        }
    }

    pub fn write_directory_metadata(
        &mut self,
        path: &Path,
        stat: FileStat,
    ) -> Result<(), VFSError> {
        let mut limits = Limits::reset();
        let entry = self.resolve_or_create_path(&mut limits, self.workdir.inode, path)?;
        let entry = self.fs.resolve_symlinks(&mut limits, entry)?;
        let inode = self.get_inode_mut(entry.child)?;
        if let Node::NormalDirectory(_) = inode.data {
            inode.stat = stat;
            Ok(())
        } else {
            Err(VFSError::DirectoryExpected)
        }
    }

    fn write_node_file(&mut self, path: &Path, stat: FileStat, data: Node) -> Result<(), VFSError> {
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
        stat: FileStat,
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

    pub fn write_shared_stream(
        &mut self,
        path: &Path,
        stat: FileStat,
        stream: SharedStream,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::SharedStream(stream))
    }

    pub fn write_symlink(
        &mut self,
        path: &Path,
        stat: FileStat,
        link_to: CString,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::SymbolicLink(link_to))
    }

    pub fn write_hardlink(&mut self, path: &Path, link_to: &Path) -> Result<(), VFSError> {
        let mut limits = Limits::reset();
        let link_to_node = self
            .fs
            .resolve_path(&mut limits, self.workdir.inode, link_to)?
            .child;
        let (dir, name) = self.resolve_or_create_parent(&mut limits, path)?;
        self.add_child_to_directory(dir, name, link_to_node)?;
        Ok(())
    }

    pub fn write_fifo(&mut self, path: &Path, stat: FileStat) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::Fifo)
    }

    pub fn write_char_device(
        &mut self,
        path: &Path,
        stat: FileStat,
        major: u32,
        minor: u32,
    ) -> Result<(), VFSError> {
        self.write_node_file(path, stat, Node::Char(major, minor))
    }

    pub fn write_block_device(
        &mut self,
        path: &Path,
        stat: FileStat,
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

fn open_null() -> Result<Arc<dyn AsRawFd + Sync + Send>, VFSError> {
    Ok(Arc::new(
        File::open("/dev/null").map_err(|_| VFSError::ImageStorageError)?,
    ))
}

async fn open_storage_part(
    storage: &FileStorage,
    key: &StorageKey,
) -> Result<Arc<dyn AsRawFd + Sync + Send>, VFSError> {
    Ok(Arc::new(
        storage
            .open_part(key)
            .await
            .ok()
            .flatten()
            .ok_or(VFSError::ImageStorageError)?,
    ))
}

struct DirectoryFileBuilder {
    buf: BufWriter<File>,
    offset: i64,
}

impl DirectoryFileBuilder {
    fn new() -> Result<Self, VFSError> {
        let memfd = memfd::MemfdOptions::default()
            .allow_sealing(true)
            .create("bandsocks-dir")
            .map_err(|_| VFSError::ImageStorageError)?;
        let buf = BufWriter::new(memfd.into_file());
        Ok(DirectoryFileBuilder { offset: 0, buf })
    }

    fn finish(self) -> Result<Arc<dyn AsRawFd + Sync + Send>, VFSError> {
        let memfd = self
            .buf
            .into_inner()
            .map_err(|_| VFSError::ImageStorageError)?;
        let memfd = memfd::Memfd::try_from_file(memfd).left().unwrap();
        memfd
            .add_seals(
                &[
                    memfd::FileSeal::SealWrite,
                    memfd::FileSeal::SealShrink,
                    memfd::FileSeal::SealGrow,
                    memfd::FileSeal::SealSeal,
                ]
                .iter()
                .cloned()
                .collect(),
            )
            .map_err(|_| VFSError::ImageStorageError)?;
        let mut memfd = memfd.into_file();
        memfd
            .seek(SeekFrom::Start(0))
            .map_err(|_| VFSError::ImageStorageError)?;
        Ok(Arc::new(memfd))
    }

    fn append(&mut self, name: &[u8], d_ino: u64, d_type: u8) -> Result<(), VFSError> {
        const ALIGN: usize = 16;
        const ZERO: [u8; 16] = [0; 16];

        let header_len = offset_of!(DirentHeader, d_name);
        let record_len = header_len + name.len() + 1;
        let padded_len = if (record_len % ALIGN) == 0 {
            record_len
        } else {
            record_len + ALIGN - (record_len % ALIGN)
        };
        let zeros_len = padded_len - header_len - name.len();
        let d_reclen: u16 = padded_len.try_into().map_err(|_| VFSError::NameTooLong)?;
        let d_off = self.offset + d_reclen as i64;

        for part in &[
            &unsafe {
                plain::as_bytes(&PlainDirentHeader(DirentHeader {
                    d_ino,
                    d_type,
                    d_off,
                    d_reclen,
                    d_name: 0,
                }))
            }[..header_len],
            name,
            &ZERO[..zeros_len],
        ] {
            self.buf
                .write_all(part)
                .map_err(|_| VFSError::ImageStorageError)?;
        }
        self.offset = d_off;
        Ok(())
    }
}
