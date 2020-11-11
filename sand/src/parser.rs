use crate::protocol::SysFd;
use core::iter::Iterator;
use heapless::{ArrayLength, Vec};
use sc::syscall;

struct ByteReader<T: ArrayLength<u8>> {
    file: Option<SysFd>,
    buf_position: usize,
    buf: Vec<u8, T>,
}

impl<T: ArrayLength<u8>> ByteReader<T> {
    fn from_sysfd(file: SysFd) -> Result<Self, ()> {
        let mut buf = Vec::new();
        unsafe {
            let len = syscall!(PREAD64, file.0, buf.as_mut_ptr(), buf.capacity(), 0) as isize;
            if len < 0 {
                return Err(());
            }
            assert!(len as usize <= buf.capacity());
            buf.set_len(len as usize);
        }
        Ok(ByteReader {
            file: Some(file),
            buf_position: 0,
            buf,
        })
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
        Ok(ByteReader {
            file: None,
            buf_position: 0,
            buf: Vec::from_slice(bytes)?,
        })
    }
}

impl<T: ArrayLength<u8>> Iterator for ByteReader<T> {
    type Item = Result<u8, ()>;
    fn next(&mut self) -> Option<Result<u8, ()>> {
        if let Some(file) = &self.file {
            if self.buf_position == self.buf.len() {
                self.buf_position = 0;
                unsafe {
                    let len = syscall!(READ, file.0, self.buf.as_mut_ptr(), self.buf.capacity(), 0)
                        as isize;
                    if len < 0 {
                        return Some(Err(()));
                    }
                    assert!(len as usize <= self.buf.capacity());
                    self.buf.set_len(len as usize);
                }
            }
        }
        if self.buf_position == self.buf.len() {
            None
        } else {
            let byte = self.buf[self.buf_position];
            self.buf_position += 1;
            Some(Ok(byte))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocol::SysFd;
    use heapless::consts::*;
    use std::{fs::File, os::unix::io::AsRawFd};

    #[test]
    fn blah() {
        let mut buf = ByteReader::<U128>::from_bytes(b"blah").unwrap();
        assert_eq!(buf.next(), Some(Ok(b'b')));
        assert_eq!(buf.next(), Some(Ok(b'l')));
        assert_eq!(buf.next(), Some(Ok(b'a')));
        assert_eq!(buf.next(), Some(Ok(b'h')));
        assert_eq!(buf.next(), None);
        assert_eq!(buf.next(), None);
    }

    #[test]
    fn devzero() {
        let f = File::open("/dev/zero").unwrap();
        let mut buf = ByteReader::<U2>::from_sysfd(SysFd(f.as_raw_fd() as u32)).unwrap();
        assert_eq!(buf.next(), Some(Ok(0)));
        assert_eq!(buf.next(), Some(Ok(0)));
        assert_eq!(buf.next(), Some(Ok(0)));
        assert_eq!(buf.next(), Some(Ok(0)));
        assert_eq!(buf.next(), Some(Ok(0)));
        assert_eq!(buf.next(), Some(Ok(0)));
    }

    #[test]
    fn devnull() {
        let f = File::open("/dev/null").unwrap();
        let mut buf = ByteReader::<U16>::from_sysfd(SysFd(f.as_raw_fd() as u32)).unwrap();
        assert_eq!(buf.next(), None);
        assert_eq!(buf.next(), None);
    }
}
