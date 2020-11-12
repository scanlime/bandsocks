use crate::protocol::SysFd;
use core::iter::Iterator;
use heapless::{ArrayLength, Vec};
use sc::syscall;

pub struct ByteReader<T: ArrayLength<u8>> {
    file: Option<SysFd>,
    file_position: usize,
    buf_position: usize,
    buf: Vec<u8, T>,
}

pub trait Stream {
    fn peek(&mut self) -> Option<Result<u8, ()>>;
    fn next(&mut self) -> Option<Result<u8, ()>>;
}

// Safe for use with seq_file instances in linux (like procfiles).
// They track the file pointer seprately, and re-generate their contents
// only when we re-read offset zero. The kernel assumes the file offsets
// increase as expected, it does not support arbitrary seeks.
impl<T: ArrayLength<u8>> ByteReader<T> {
    pub fn from_sysfd(file: SysFd) -> Result<Self, ()> {
        Ok(ByteReader {
            file: Some(file),
            file_position: 0,
            buf_position: 0,
            buf: Vec::new(),
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
        Ok(ByteReader {
            file: None,
            file_position: 0,
            buf_position: 0,
            buf: Vec::from_slice(bytes)?,
        })
    }
}

impl<T: ArrayLength<u8>> Stream for ByteReader<T> {
    fn peek(&mut self) -> Option<Result<u8, ()>> {
        if let Some(file) = &self.file {
            if self.buf_position == self.buf.len() {
                self.buf_position = 0;
                unsafe {
                    let len = syscall!(
                        PREAD64,
                        file.0,
                        self.buf.as_mut_ptr(),
                        self.buf.capacity(),
                        self.file_position
                    ) as isize;
                    if len < 0 {
                        return Some(Err(()));
                    }
                    assert!(len as usize <= self.buf.capacity());
                    self.buf.set_len(len as usize);
                }
                self.file_position += self.buf.len();
                if self.buf.is_empty() {
                    self.file.take();
                }
            }
        }
        if self.buf_position == self.buf.len() {
            None
        } else {
            let byte = self.buf[self.buf_position];
            Some(Ok(byte))
        }
    }

    fn next(&mut self) -> Option<Result<u8, ()>> {
        match self.peek() {
            Some(Ok(byte)) => {
                self.buf_position += 1;
                Some(Ok(byte))
            }
            Some(Err(())) => Some(Err(())),
            None => None,
        }
    }
}

pub fn byte<T: Stream>(s: &mut T, template: u8) -> Result<(), ()> {
    match s.peek() {
        Some(Ok(byte)) if byte == template => {
            s.next();
            Ok(())
        }
        _ => Err(()),
    }
}

pub fn bytes<T: Stream>(s: &mut T, template: &[u8]) -> Result<(), ()> {
    for b in template {
        byte(s, *b)?;
    }
    Ok(())
}

pub fn space<T: Stream>(s: &mut T) -> Result<(), ()> {
    byte(s, b' ').or_else(|_| byte(s, b'\t'))
}

pub fn spaces<T: Stream>(s: &mut T) -> Result<(), ()> {
    space(s)?;
    while space(s).is_ok() {}
    Ok(())
}

pub fn eof<T: Stream>(s: &mut T) -> Result<(), ()> {
    match s.peek() {
        None => Ok(()),
        _ => Err(()),
    }
}

pub fn u64_dec<T: Stream>(s: &mut T) -> Result<u64, ()> {
    match s.peek() {
        Some(Ok(byte)) if (b'0'..=b'9').contains(&byte) => Ok({
            let mut value = 0;
            while let Some(Ok(byte)) = s.peek() {
                let digit = (byte as u64).wrapping_sub(b'0' as u64);
                if digit < 10 {
                    s.next();
                    value = value * 10 + digit;
                } else {
                    break;
                }
            }
            value
        }),
        _ => Err(()),
    }
}

pub fn u64_0x<T: Stream>(s: &mut T) -> Result<u64, ()> {
    bytes(s, b"0x").and_then(|_| u64_hex(s))
}

pub fn u64_hex<T: Stream>(s: &mut T) -> Result<u64, ()> {
    match s.peek() {
        Some(Ok(byte))
            if (b'0'..=b'9').contains(&byte)
                || (b'a'..=b'f').contains(&byte)
                || (b'A'..=b'F').contains(&byte) =>
        {
            Ok({
                let mut value = 0;
                while let Some(Ok(byte)) = s.peek() {
                    let digit = match byte {
                        b'0'..=b'9' => byte - b'0',
                        b'a'..=b'f' => 10 + byte - b'a',
                        b'A'..=b'F' => 10 + byte - b'A',
                        _ => break,
                    };
                    s.next();
                    value = (value << 4) | digit as u64;
                }
                value
            })
        }
        _ => Err(()),
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
        let mut r = ByteReader::<U128>::from_bytes(b"blah").unwrap();
        assert_eq!(r.next(), Some(Ok(b'b')));
        assert_eq!(r.next(), Some(Ok(b'l')));
        assert_eq!(r.next(), Some(Ok(b'a')));
        assert_eq!(r.next(), Some(Ok(b'h')));
        assert_eq!(r.next(), None);
        assert_eq!(r.next(), None);
    }

    #[test]
    fn blah2() {
        let mut r = ByteReader::<U128>::from_bytes(b"blah").unwrap();
        assert_eq!(byte(&mut r, b'b'), Ok(()));
        assert_eq!(byte(&mut r, b'b'), Err(()));
        assert_eq!(eof(&mut r), Err(()));
        assert_eq!(byte(&mut r, b'l'), Ok(()));
        assert_eq!(eof(&mut r), Err(()));
        assert_eq!(byte(&mut r, b'a'), Ok(()));
        assert_eq!(byte(&mut r, b'a'), Err(()));
        assert_eq!(byte(&mut r, b'h'), Ok(()));
        assert_eq!(eof(&mut r), Ok(()));
        assert_eq!(byte(&mut r, b'a'), Err(()));
        assert_eq!(eof(&mut r), Ok(()));
    }

    #[test]
    fn dev_zero() {
        let f = File::open("/dev/zero").unwrap();
        let mut r = ByteReader::<U2>::from_sysfd(SysFd(f.as_raw_fd() as u32)).unwrap();
        assert_eq!(r.next(), Some(Ok(0)));
        assert_eq!(r.next(), Some(Ok(0)));
        assert_eq!(r.next(), Some(Ok(0)));
        assert_eq!(r.next(), Some(Ok(0)));
        assert_eq!(r.next(), Some(Ok(0)));
        assert_eq!(r.next(), Some(Ok(0)));
    }

    #[test]
    fn dev_null() {
        let f = File::open("/dev/null").unwrap();
        let mut r = ByteReader::<U16>::from_sysfd(SysFd(f.as_raw_fd() as u32)).unwrap();
        assert_eq!(r.next(), None);
        assert_eq!(r.next(), None);
    }

    #[test]
    fn proc_atomicity() {
        let f = File::open("/proc/thread-self/syscall").unwrap();
        let mut r = ByteReader::<U1>::from_sysfd(SysFd(f.as_raw_fd() as u32)).unwrap();
        let syscall_nr = u64_dec(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let arg_1 = u64_0x(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let arg_2 = u64_0x(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let arg_3 = u64_0x(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let arg_4 = u64_0x(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let _arg_5 = u64_0x(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let _arg_6 = u64_0x(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let _sp = u64_0x(&mut r).unwrap();
        spaces(&mut r).unwrap();
        let _pc = u64_0x(&mut r).unwrap();
        byte(&mut r, b'\n').unwrap();
        eof(&mut r).unwrap();
        assert_eq!(sc::nr::PREAD64 as u64, syscall_nr);
        assert_eq!(arg_1, f.as_raw_fd() as u64);
        assert_eq!(arg_2, r.buf.as_ptr() as usize as u64);
        assert_eq!(arg_3, 1);
        assert_eq!(arg_4, 0);
    }
}
