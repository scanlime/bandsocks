use crate::{
    mem::{
        maps::{MappedPages, MappedRange, MemFlags, MemProtect},
        page::VPage,
        rw::find_syscall,
    },
    nolibc::File,
    parser,
    parser::{ByteReader, Token},
    process::task::StoppedTask,
    protocol::VPtr,
};
use core::{fmt, iter::Iterator, marker::PhantomData};
use typenum::*;

pub type MapsBufferSize = U512;

pub struct KernelMemIterator<'q, 's, 't> {
    stopped_task: PhantomData<&'t mut StoppedTask<'q, 's>>,
    stream: ByteReader<MapsBufferSize>,
}

impl<'q, 's, 't> KernelMemIterator<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let maps_file = File::new(stopped_task.task.process_handle.maps);
        let stream = ByteReader::from_file(maps_file);
        KernelMemIterator {
            stopped_task: PhantomData,
            stream,
        }
    }

    pub fn print_maps(stopped_task: &'t mut StoppedTask<'q, 's>) {
        println!("maps dump:");
        for area in KernelMemIterator::new(stopped_task) {
            println!("{:x?}", area);
        }
    }
}

impl<'q, 's, 't> Iterator for KernelMemIterator<'q, 's, 't> {
    type Item = KernelMemArea;

    fn next(&mut self) -> Option<KernelMemArea> {
        if parser::eof(&mut self.stream).is_ok() {
            None
        } else {
            let mem = {
                let start = VPtr(parser::u64_hex(&mut self.stream).unwrap() as usize);
                parser::byte(&mut self.stream, b'-').unwrap();
                let end = VPtr(parser::u64_hex(&mut self.stream).unwrap() as usize);
                parser::spaces(&mut self.stream).unwrap();
                start..end
            };

            let flags = {
                let read = parser::flag(&mut self.stream, b'r', b'-').unwrap();
                let write = parser::flag(&mut self.stream, b'w', b'-').unwrap();
                let execute = parser::flag(&mut self.stream, b'x', b'-').unwrap();
                let mayshare = parser::flag(&mut self.stream, b's', b'p').unwrap();
                parser::spaces(&mut self.stream).unwrap();
                MemFlags {
                    protect: MemProtect {
                        read,
                        write,
                        execute,
                    },
                    mayshare,
                }
            };

            let file_start = parser::u64_hex(&mut self.stream).unwrap() as usize;
            parser::spaces(&mut self.stream).unwrap();

            let file = {
                let dev_major = parser::u64_hex(&mut self.stream).unwrap() as usize;
                parser::byte(&mut self.stream, b':').unwrap();
                let dev_minor = parser::u64_hex(&mut self.stream).unwrap() as usize;
                parser::spaces(&mut self.stream).unwrap();
                let inode = parser::u64_dec(&mut self.stream).unwrap() as usize;
                parser::spaces(&mut self.stream).unwrap();
                KernelFile {
                    device: KernelDevice(dev_major, dev_minor),
                    inode,
                }
            };

            let name = match parser::switch(
                &mut self.stream,
                &mut [
                    Token::new(b"/", &KernelMemName::Path),
                    Token::new(b"[vdso]\n", &KernelMemName::VDSO),
                    Token::new(b"[vvar]\n", &KernelMemName::VVar),
                    Token::new(b"[vsyscall]\n", &KernelMemName::VSyscall),
                    Token::new(b"\n", &KernelMemName::None),
                ],
            ) {
                Ok(name) if name == &KernelMemName::Path => {
                    parser::until_byte_inclusive(&mut self.stream, b'\n').unwrap();
                    KernelMemName::Path
                }
                Err(()) => {
                    parser::until_byte_inclusive(&mut self.stream, b'\n').unwrap();
                    KernelMemName::Other
                }
                Ok(name) => name.clone(),
            };

            Some(KernelMemArea {
                pages: MappedPages::parse(&MappedRange { mem, file_start })
                    .expect("page aligned kernel memory area"),
                name,
                flags,
                file,
            })
        }
    }
}

#[derive(Debug)]
pub struct KernelMemAreas {
    pub vdso: KernelMemArea,
    pub vvar: KernelMemArea,
    pub vsyscall: Option<KernelMemArea>,
    pub vdso_syscall: VPtr,
    pub task_end: VPage,
}

#[derive(Eq, PartialEq, Clone)]
pub struct KernelDevice(usize, usize);

impl fmt::Debug for KernelDevice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}:{:?}", self.0, self.1)
    }
}

#[derive(Eq, PartialEq, Clone)]
pub struct KernelFile {
    device: KernelDevice,
    inode: usize,
}

impl KernelFile {
    pub fn null() -> Self {
        KernelFile {
            device: KernelDevice(0, 0),
            inode: 0,
        }
    }
}

impl fmt::Debug for KernelFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}:{:?}", self.device, self.inode)
    }
}

#[derive(Eq, PartialEq, Clone)]
pub struct KernelMemArea {
    pub name: KernelMemName,
    pub pages: MappedPages,
    pub flags: MemFlags,
    pub file: KernelFile,
}

impl KernelMemArea {
    pub fn is_overlap(&self, other: &Self) -> bool {
        self.name == other.name
            && self.flags == other.flags
            && self.file == other.file
            && self.pages.is_overlap(&other.pages)
    }
}

impl fmt::Debug for KernelMemArea {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "KernelMemArea({:?} {:?} {:?} {:?})",
            self.flags, self.pages, self.name, self.file
        )
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum KernelMemName {
    None,
    VDSO,
    VVar,
    VSyscall,
    Path,
    Other,
}

impl KernelMemAreas {
    pub fn locate(stopped_task: &mut StoppedTask) -> Self {
        let mut vdso = None;
        let mut vvar = None;
        let mut vsyscall = None;
        let mut task_end = VPage::max();

        for map in KernelMemIterator::new(stopped_task) {
            match map.name {
                KernelMemName::VDSO => {
                    assert_eq!(map.flags, MemFlags::exec());
                    assert_eq!(map.file, KernelFile::null());
                    assert_eq!(vdso, None);
                    task_end = task_end.min(map.pages.mem_pages().start);
                    vdso = Some(map);
                }
                KernelMemName::VVar => {
                    assert_eq!(map.flags, MemFlags::ro());
                    assert_eq!(map.file, KernelFile::null());
                    assert_eq!(vvar, None);
                    task_end = task_end.min(map.pages.mem_pages().start);
                    vvar = Some(map);
                }
                KernelMemName::VSyscall => {
                    assert_eq!(map.flags.protect.write, false);
                    assert_eq!(map.flags.protect.execute, true);
                    assert_eq!(map.flags.mayshare, false);
                    assert_eq!(map.file, KernelFile::null());
                    assert_eq!(map.file, KernelFile::null());
                    assert_eq!(vsyscall, None);
                    task_end = task_end.min(map.pages.mem_pages().start);
                    vsyscall = Some(map);
                }
                _ => {}
            }
        }

        let vdso = vdso.unwrap();
        let vvar = vvar.unwrap();
        let vdso_syscall = find_syscall(stopped_task, vdso.pages.mem_range()).unwrap();

        KernelMemAreas {
            vdso,
            vvar,
            vsyscall,
            vdso_syscall,
            task_end,
        }
    }

    pub fn is_userspace_area(&self, area: &KernelMemArea) -> bool {
        // This tests for overlap (including identical device and name) rather than
        // strict equality, since vvar can change size due to linux timer
        // namespaces
        if area.is_overlap(&self.vdso) {
            return false;
        }
        if area.is_overlap(&self.vvar) {
            return false;
        }
        if let Some(vsyscall) = self.vsyscall.as_ref() {
            if area.is_overlap(vsyscall) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{nolibc::File, protocol::SysFd};
    use std::{fs, os::unix::io::AsRawFd};

    #[test]
    fn self_maps() {
        let std_file = fs::File::open("/proc/thread-self/maps").unwrap();
        let nolibc_file = File::new(SysFd(std_file.as_raw_fd() as u32));
        let r = ByteReader::<MapsBufferSize>::from_file(nolibc_file);
        let iter = KernelMemIterator {
            stopped_task: PhantomData,
            stream: r,
        };
        let mut found_vdso = false;
        for map in iter {
            if map.name == KernelMemName::VDSO {
                assert_eq!(map.flags, MemFlags::exec());
                assert_eq!(
                    map.file,
                    KernelFile {
                        device: KernelDevice(0, 0),
                        inode: 0
                    }
                );
                assert_eq!(found_vdso, false);
                found_vdso = true;
            }
        }
        assert_eq!(found_vdso, true);
    }
}
