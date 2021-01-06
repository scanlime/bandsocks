use crate::{
    abi,
    mem::rw::read_pointer,
    nolibc::File,
    parser::{ByteReader, Stream},
    process::task::StoppedTask,
    protocol::{Errno, VPtr, VString},
};
use core::{mem::size_of, ops::Range};
use typenum::*;

#[derive(Debug)]
pub struct VStringArray(pub VPtr);

impl VStringArray {
    pub fn array_ptr(&self) -> VPtr {
        self.0
    }

    pub fn item_ptr(
        &self,
        stopped_task: &mut StoppedTask,
        idx: usize,
    ) -> Result<Option<VString>, Errno> {
        match read_pointer(stopped_task, self.array_ptr() + (idx * size_of::<VPtr>())) {
            Err(err) => Err(err),
            Ok(ptr) if ptr == VPtr::null() => Ok(None),
            Ok(ptr) => Ok(Some(VString(ptr))),
        }
    }

    pub fn item_range(
        &self,
        stopped_task: &mut StoppedTask,
        idx: usize,
    ) -> Result<Option<VStringRange>, Errno> {
        match self.item_ptr(stopped_task, idx) {
            Err(err) => Err(err),
            Ok(None) => Ok(None),
            Ok(Some(ptr)) => Ok(Some(VStringRange::parse(stopped_task, ptr)?)),
        }
    }
}

pub struct VStringRange(Range<VPtr>);

impl VStringRange {
    pub fn range(&self) -> Range<VPtr> {
        self.0.clone()
    }

    pub fn parse(stopped_task: &mut StoppedTask, vstring: VString) -> Result<VStringRange, Errno> {
        // Use small read buffers that don't cross page boundaries
        type BufSize = U128;
        let ptr = vstring.0;
        let alignment = ptr.0 % BufSize::USIZE;
        let mem_file = File::new(stopped_task.task.process_handle.mem);
        let mut buf = ByteReader::<BufSize>::from_file_at(mem_file, ptr.0 - alignment);
        for _ in 0..alignment {
            match buf.next() {
                Some(Ok(_byte)) => (),
                _ => return Err(Errno(-abi::EFAULT)),
            }
        }
        let mut len = 0;
        while let Some(Ok(byte)) = buf.next() {
            len += 1;
            if byte == 0 {
                return Ok(VStringRange(ptr..(ptr + len)));
            }
        }
        Err(Errno(-abi::EFAULT))
    }
}
