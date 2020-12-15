use crate::{
    nolibc::File,
    parser,
    parser::{ByteReader, Token},
    process::task::StoppedTask,
    protocol::VPtr,
    remote::mem::find_bytes,
};
use core::{iter::Iterator, marker::PhantomData};
use heapless::consts::*;

#[derive(Eq, PartialEq, Clone)]
pub struct MemArea {
    pub start: usize,
    pub end: usize,
    pub offset: usize,
    pub dev_major: usize,
    pub dev_minor: usize,
    pub inode: usize,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub mayshare: bool,
    pub name: MemAreaName,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum MemAreaName {
    None,
    VDSO,
    VVar,
    VSyscall,
    Path,
    Other,
}

impl MemArea {
    pub fn vptr(&self) -> VPtr {
        VPtr(self.start)
    }

    pub fn len(&self) -> usize {
        assert!(self.end >= self.start);
        self.end - self.start + 1
    }

    pub fn is_overlap(&self, other: &Self) -> bool {
        self.name == other.name
            && self.read == other.read
            && self.write == other.write
            && self.execute == other.execute
            && self.mayshare == other.mayshare
            && self.dev_major == other.dev_major
            && self.dev_minor == other.dev_minor
            && self.inode == other.inode
            && self.end > self.start
            && other.end > other.start
            && self.end.min(other.end) >= self.start.max(other.start)
    }
}

impl core::fmt::Debug for MemArea {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "MemArea({:16x}-{:16x} {}{}{}{} {:?} {}:{}:{}@{:x})",
            self.start,
            self.end,
            if self.read { "r" } else { "-" },
            if self.write { "w" } else { "-" },
            if self.execute { "x" } else { "-" },
            if self.mayshare { "s" } else { "p" },
            self.name,
            self.dev_major,
            self.dev_minor,
            self.inode,
            self.offset,
        )
    }
}

// This buffer does not need to be large, for performance it should typically
// hold a couple map entries though.
type MapsBufferSize = U512;

pub struct MapsIterator<'q, 's, 't> {
    stopped_task: PhantomData<&'t mut StoppedTask<'q, 's>>,
    stream: ByteReader<MapsBufferSize>,
}

impl<'q, 's, 't> MapsIterator<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let maps_file = File::new(stopped_task.task.process_handle.maps);
        let stream = ByteReader::from_file(maps_file);
        MapsIterator {
            stopped_task: PhantomData,
            stream,
        }
    }
}

impl<'q, 's, 't> Iterator for MapsIterator<'q, 's, 't> {
    type Item = MemArea;

    fn next(&mut self) -> Option<MemArea> {
        if parser::eof(&mut self.stream).is_ok() {
            None
        } else {
            let start = parser::u64_hex(&mut self.stream).unwrap() as usize;
            parser::byte(&mut self.stream, b'-').unwrap();
            let end = parser::u64_hex(&mut self.stream).unwrap() as usize;
            parser::spaces(&mut self.stream).unwrap();

            let read = parser::flag(&mut self.stream, b'r', b'-').unwrap();
            let write = parser::flag(&mut self.stream, b'w', b'-').unwrap();
            let execute = parser::flag(&mut self.stream, b'x', b'-').unwrap();
            let mayshare = parser::flag(&mut self.stream, b's', b'p').unwrap();
            parser::spaces(&mut self.stream).unwrap();

            let offset = parser::u64_hex(&mut self.stream).unwrap() as usize;
            parser::spaces(&mut self.stream).unwrap();

            let dev_major = parser::u64_hex(&mut self.stream).unwrap() as usize;
            parser::byte(&mut self.stream, b':').unwrap();
            let dev_minor = parser::u64_hex(&mut self.stream).unwrap() as usize;
            parser::spaces(&mut self.stream).unwrap();

            let inode = parser::u64_dec(&mut self.stream).unwrap() as usize;
            parser::spaces(&mut self.stream).unwrap();

            let name = match parser::switch(
                &mut self.stream,
                &mut [
                    Token::new(b"/", &MemAreaName::Path),
                    Token::new(b"[vdso]\n", &MemAreaName::VDSO),
                    Token::new(b"[vvar]\n", &MemAreaName::VVar),
                    Token::new(b"[vsyscall]\n", &MemAreaName::VSyscall),
                    Token::new(b"\n", &MemAreaName::None),
                ],
            ) {
                Ok(name) if name == &MemAreaName::Path => {
                    parser::until_byte_inclusive(&mut self.stream, b'\n').unwrap();
                    MemAreaName::Path
                }
                Err(()) => {
                    parser::until_byte_inclusive(&mut self.stream, b'\n').unwrap();
                    MemAreaName::Other
                }
                Ok(name) => name.clone(),
            };

            Some(MemArea {
                start,
                end,
                read,
                write,
                execute,
                mayshare,
                offset,
                dev_major,
                dev_minor,
                inode,
                name,
            })
        }
    }
}

pub fn print_maps_dump(stopped_task: &mut StoppedTask) {
    println!("maps dump:");
    for area in MapsIterator::new(stopped_task) {
        println!("{:x?}", area);
    }
}

#[derive(Debug)]
pub struct KernelMemAreas {
    pub vdso: MemArea,
    pub vvar: MemArea,
    pub vsyscall: Option<MemArea>,
    pub vdso_syscall: VPtr,
    pub task_end: VPtr,
}

fn find_syscall<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    vdso: &MemArea,
) -> Result<VPtr, ()> {
    const X86_64_SYSCALL: [u8; 2] = [0x0f, 0x05];
    find_bytes(
        stopped_task,
        VPtr(vdso.start),
        vdso.end - vdso.start,
        &X86_64_SYSCALL,
    )
}

impl KernelMemAreas {
    pub fn locate(stopped_task: &mut StoppedTask<'_, '_>) -> Self {
        let mut vdso = None;
        let mut vvar = None;
        let mut vsyscall = None;
        let mut task_end = !0usize;

        for map in MapsIterator::new(stopped_task) {
            match map.name {
                MemAreaName::VDSO => {
                    assert_eq!(map.read, true);
                    assert_eq!(map.write, false);
                    assert_eq!(map.execute, true);
                    assert_eq!(map.mayshare, false);
                    assert_eq!(map.dev_major, 0);
                    assert_eq!(map.dev_minor, 0);
                    assert_eq!(vdso, None);
                    task_end = task_end.min(map.start);
                    vdso = Some(map);
                }
                MemAreaName::VVar => {
                    assert_eq!(map.read, true);
                    assert_eq!(map.write, false);
                    assert_eq!(map.execute, false);
                    assert_eq!(map.mayshare, false);
                    assert_eq!(map.dev_major, 0);
                    assert_eq!(map.dev_minor, 0);
                    assert_eq!(vvar, None);
                    task_end = task_end.min(map.start);
                    vvar = Some(map);
                }
                MemAreaName::VSyscall => {
                    assert_eq!(map.write, false);
                    assert_eq!(map.execute, true);
                    assert_eq!(map.mayshare, false);
                    assert_eq!(map.dev_major, 0);
                    assert_eq!(map.dev_minor, 0);
                    assert_eq!(vsyscall, None);
                    task_end = task_end.min(map.start);
                    vsyscall = Some(map);
                }
                _ => {}
            }
        }

        let vdso = vdso.unwrap();
        let vvar = vvar.unwrap();
        let vdso_syscall = find_syscall(stopped_task, &vdso).unwrap();
        let task_end = VPtr(task_end);

        KernelMemAreas {
            vdso,
            vvar,
            vsyscall,
            vdso_syscall,
            task_end,
        }
    }

    pub fn is_userspace_area(&self, area: &MemArea) -> bool {
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
        let iter = MapsIterator {
            stopped_task: PhantomData,
            stream: r,
        };
        let mut found_vdso = false;
        for map in iter {
            if map.name == MemAreaName::VDSO {
                assert_eq!(map.execute, true);
                assert_eq!(map.read, true);
                assert_eq!(map.write, false);
                assert_eq!(map.mayshare, false);
                assert_eq!(map.dev_major, 0);
                assert_eq!(map.dev_minor, 0);
                assert_eq!(found_vdso, false);
                found_vdso = true;
            }
        }
        assert_eq!(found_vdso, true);
    }
}
