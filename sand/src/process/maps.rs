use crate::{abi, process::task::StoppedTask};
use core::iter::Iterator;
use heapless::{consts::*, Vec};
use sc::syscall;

#[derive(Debug)]
pub struct MemArea<'i> {
    pub start: usize,
    pub end: usize,
    pub offset: usize,
    pub dev: usize,
    pub inode: usize,
    pub read: bool,
    pub write: bool,
    pub exec: bool,
    pub mayshare: bool,
    pub name: Option<&'i str>,
}

// must be able to hold the longest map line (including name) that we will read
type BufferSize = U16384;

pub struct MapsIterator<'q, 's, 't> {
    stopped_task: &'t mut StoppedTask<'q, 's>,
    buffer: Vec<u8, BufferSize>,
    offset: usize,
}

impl<'q, 's, 't> MapsIterator<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        assert_eq!(0, unsafe {
            syscall!(
                LSEEK,
                stopped_task.task.process_handle.maps.0,
                0,
                abi::SEEK_SET
            )
        });
        MapsIterator {
            stopped_task,
            buffer: Vec::new(),
            offset: 0,
        }
    }

    fn fill_buffer(&mut self) {
        assert_eq!(self.offset, self.buffer.len());
        unsafe {
            let len = syscall!(
                READ,
                self.stopped_task.task.process_handle.maps.0,
                self.buffer.as_mut_ptr().add(self.buffer.len()),
                self.buffer.capacity() - self.buffer.len()
            ) as isize;
            assert!(len >= 0 && (len as usize + self.buffer.len() <= self.buffer.capacity()));
            self.buffer.set_len(len as usize);
        }
        self.offset = 0;
    }
}

impl<'i, 'q, 's, 't> Iterator for &'i mut MapsIterator<'q, 's, 't> {
    type Item = MemArea<'i>;

    fn next(&mut self) -> Option<MemArea<'i>> {
        None
    }
}
