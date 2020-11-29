use crate::{
    parser,
    parser::{ByteReader, Token},
    process::task::StoppedTask,
    protocol::VPtr,
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
}

impl core::fmt::Debug for MemArea {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "MemArea({:10?} {:16x}-{:16x} {}{}{}{} {}:{}:{}@{:x})",
            self.name,
            self.start,
            self.end,
            if self.read { "r" } else { "-" },
            if self.write { "w" } else { "-" },
            if self.execute { "x" } else { "-" },
            if self.mayshare { "s" } else { "p" },
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
    stream: ByteReader<'t, MapsBufferSize>,
}

impl<'q, 's, 't> MapsIterator<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let stream = ByteReader::from_sysfd(&stopped_task.task.process_handle.maps);
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

#[allow(dead_code)]
pub fn print_maps_dump(stopped_task: &mut StoppedTask) {
    println!("maps dump:");
    for area in MapsIterator::new(stopped_task) {
        println!("{:x?}", area);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocol::SysFd;
    use std::{fs::File, os::unix::io::AsRawFd};

    #[test]
    fn self_maps() {
        let f = File::open("/proc/thread-self/maps").unwrap();
        let sys_fd = SysFd(f.as_raw_fd() as u32);
        let r = ByteReader::<MapsBufferSize>::from_sysfd(&sys_fd);
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
