//! Double-ended double-queue, for converting between IPC messages and bytes
//! plus files

use super::{de, ser, SysFd};
use core::{fmt, ops::Range};
use generic_array::{typenum::*, ArrayLength, GenericArray};
use serde::{de::DeserializeOwned, Serialize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unimplemented,
    UnexpectedEnd,
    BufferFull,
    InvalidValue,
    Serialize,
    Deserialize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub type Result<T> = core::result::Result<T, Error>;
pub type BytesMax = U4096;
pub type FilesMax = U128;

#[derive(Default)]
pub struct IPCBuffer {
    bytes: Queue<u8, BytesMax>,
    files: Queue<SysFd, FilesMax>,
}

#[derive(Default)]
struct Queue<T: Clone, N: ArrayLength<T>> {
    array: GenericArray<T, N>,
    range: Range<usize>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct IPCSlice<'a> {
    pub bytes: &'a [u8],
    pub files: &'a [SysFd],
}

#[derive(Debug, Eq, PartialEq)]
pub struct IPCSliceMut<'a> {
    pub bytes: &'a mut [u8],
    pub files: &'a mut [SysFd],
}

impl<T: Copy, N: ArrayLength<T>> Queue<T, N> {
    fn is_empty(&self) -> bool {
        self.range.is_empty()
    }

    fn push_back(&mut self, item: T) -> Result<()> {
        if self.range.end < self.array.len() {
            self.array[self.range.end] = item;
            self.range.end += 1;
            Ok(())
        } else {
            Err(Error::BufferFull)
        }
    }

    fn extend(&mut self, items: &[T]) -> Result<()> {
        let new_end = self.range.end + items.len();
        if new_end > self.array.len() {
            Err(Error::BufferFull)
        } else {
            self.array[self.range.end..new_end].clone_from_slice(items);
            self.range.end = new_end;
            Ok(())
        }
    }

    fn pop_front(&mut self, count: usize) {
        self.range.start += count;
        assert!(self.range.start <= self.range.end);
    }

    fn as_slice(&self) -> &[T] {
        &self.array[self.range.clone()]
    }

    fn begin_fill(&mut self) -> &mut [T] {
        let prev_partial_range = self.range.clone();
        let new_partial_range = 0..prev_partial_range.end - prev_partial_range.start;
        let new_empty_range = new_partial_range.end..self.array.len();
        self.array.copy_within(prev_partial_range, 0);
        self.range = new_partial_range;
        &mut self.array[new_empty_range]
    }

    fn commit_fill(&mut self, len: usize) {
        let new_end = self.range.end + len;
        assert!(new_end <= self.array.len());
        self.range.end = new_end;
    }

    fn front(&self, len: usize) -> Result<&[T]> {
        let slice = self.as_slice();
        if len <= slice.len() {
            Ok(&slice[..len])
        } else {
            Err(Error::UnexpectedEnd)
        }
    }
}

impl<'a> IPCBuffer {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn as_slice(&'a self) -> IPCSlice<'a> {
        IPCSlice {
            bytes: self.bytes.as_slice(),
            files: self.files.as_slice(),
        }
    }

    pub fn begin_fill(&'a mut self) -> IPCSliceMut<'a> {
        IPCSliceMut {
            bytes: self.bytes.begin_fill(),
            files: self.files.begin_fill(),
        }
    }

    pub fn commit_fill(&'a mut self, num_bytes: usize, num_files: usize) {
        self.bytes.commit_fill(num_bytes);
        self.files.commit_fill(num_files);
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty() && self.files.is_empty()
    }

    pub fn push_back<T: Serialize>(&mut self, message: &T) -> Result<()> {
        let mut serializer = ser::IPCSerializer::new(self);
        message.serialize(&mut serializer)
    }

    pub fn pop_front<T: Clone + DeserializeOwned>(&'a mut self) -> Result<T> {
        let saved_bytes_range = self.bytes.range.clone();
        let saved_files_range = self.files.range.clone();
        let mut deserializer = de::IPCDeserializer::new(self);
        let result = T::deserialize(&mut deserializer);
        if result.is_err() {
            // Rewind the pop on error, to recover after a partial read
            self.bytes.range = saved_bytes_range;
            self.files.range = saved_files_range;
        }
        result
    }

    pub fn extend_bytes(&mut self, data: &[u8]) -> Result<()> {
        self.bytes.extend(data)
    }

    pub fn push_back_byte(&mut self, data: u8) -> Result<()> {
        self.bytes.push_back(data)
    }

    pub fn push_back_file(&mut self, file: SysFd) -> Result<()> {
        self.files.push_back(file)
    }

    pub fn front_bytes(&self, len: usize) -> Result<&[u8]> {
        self.bytes.front(len)
    }

    pub fn front_files(&self, len: usize) -> Result<&[SysFd]> {
        self.files.front(len)
    }

    pub fn pop_front_bytes(&mut self, len: usize) {
        self.bytes.pop_front(len)
    }

    pub fn pop_front_files(&mut self, len: usize) {
        self.files.pop_front(len)
    }

    pub fn pop_front_byte(&mut self) -> Result<u8> {
        let result = self.front_bytes(1)?[0];
        self.pop_front_bytes(1);
        Ok(result)
    }

    pub fn pop_front_file(&mut self) -> Result<SysFd> {
        let result = self.front_files(1)?[0];
        self.pop_front_files(1);
        Ok(result)
    }
}
