use crate::{
    abi,
    abi::LinuxDirentHeader,
    protocol::{Errno, SysFd},
};
use core::{fmt, mem, mem::size_of, slice, str};
use heapless::{ArrayLength, Vec};
use plain::Plain;
use sc::syscall;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (
        $crate::write_stderr(core::format_args!( $($arg)* ))
    );
}

#[macro_export]
macro_rules! println {
    () => ({
        $crate::print!("\n");
    });
    ($($arg:tt)*) => ({
        $crate::print!( $($arg)* );
        $crate::println!();
    });
}

pub const PROC_SELF_EXE: &[u8] = b"/proc/self/exe\0";
pub const PROC_SELF_FD: &[u8] = b"/proc/self/fd\0";

pub const EXIT_SUCCESS: usize = 0;
pub const EXIT_PANIC: usize = 10;
pub const EXIT_IO_ERROR: usize = 20;

impl fmt::Write for SysFd {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() == unsafe { syscall!(WRITE, self.0, s.as_ptr() as usize, s.len()) } {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

impl SysFd {
    pub fn close(&self) -> Result<(), Errno> {
        let result = unsafe { syscall!(CLOSE, self.0) as isize };
        if result == 0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
        }
    }

    pub fn read_exact(&self, bytes: &mut [u8]) -> Result<(), Errno> {
        let mut offset = 0;
        while offset < bytes.len() {
            let slice = &mut bytes[offset..];
            let result = unsafe {
                syscall!(READ, self.0, slice.as_mut_ptr() as usize, slice.len()) as isize
            };
            if result <= 0 {
                return Err(Errno(result as i32));
            } else {
                offset += result as usize;
            }
        }
        Ok(())
    }
}

pub fn getpid() -> usize {
    unsafe { syscall!(GETPID) }
}

pub fn exit(code: usize) -> ! {
    unsafe { syscall!(EXIT, code) };
    unreachable!()
}

pub fn socketpair(domain: usize, ty: usize, protocol: usize) -> Result<(SysFd, SysFd), Errno> {
    let mut pair = [0u32; 4];
    let result = unsafe { syscall!(SOCKETPAIR, domain, ty, protocol, pair.as_mut_ptr()) as isize };
    if result == 0 {
        Ok((SysFd(pair[0]), SysFd(pair[1])))
    } else {
        Err(Errno(result as i32))
    }
}

pub fn stderr() -> SysFd {
    SysFd(2)
}

pub fn write_stderr(msg: fmt::Arguments) {
    if fmt::write(&mut stderr(), msg).is_err() {
        exit(EXIT_IO_ERROR);
    }
}

/// # Safety
/// Pointer is to a nul terminated C string
pub unsafe fn c_strlen(s: *const u8) -> usize {
    let mut count: usize = 0;
    while *s.add(count) != 0 {
        count += 1;
    }
    count
}

pub fn c_unwrap_nul(s: &[u8]) -> &[u8] {
    assert_eq!(s.last(), Some(&0u8));
    &s[0..s.len() - 1]
}

/// # Safety
/// Pointer is to a nul terminated C string with static lifetime
pub unsafe fn c_str_slice(s: *const u8) -> &'static [u8] {
    slice::from_raw_parts(s, 1 + c_strlen(s))
}

/// # Safety
/// Pointer is to a null terminated array
pub unsafe fn c_strv_len(strv: *const *const u8) -> usize {
    let mut count: usize = 0;
    while !(*strv.add(count)).is_null() {
        count += 1;
    }
    count
}

/// # Safety
/// Pointer is to a null terminated array with static lifetime
pub unsafe fn c_strv_slice(strv: *const *const u8) -> &'static [*const u8] {
    slice::from_raw_parts(strv, c_strv_len(strv))
}

#[naked]
unsafe extern "C" fn sigreturn() {
    syscall!(RT_SIGRETURN);
    unreachable!();
}

/// # Safety
/// Needs a NUL-terminated C string.
/// Also note this will fail if run after the seccomp policy is installed.
pub unsafe fn open(name: &[u8], flags: usize, mode: usize) -> Result<SysFd, Errno> {
    match syscall!(OPEN, name.as_ptr(), flags, mode) as isize {
        result if result >= 0 => Ok(SysFd(result as u32)),
        err => Err(Errno(err as i32)),
    }
}

pub fn signal(signum: u32, handler: extern "C" fn(u32)) -> Result<(), Errno> {
    let sigaction = abi::SigAction {
        sa_flags: abi::SA_RESTORER,
        sa_handler: handler,
        sa_restorer: sigreturn,
        sa_mask: [0; 16],
    };
    match unsafe {
        syscall!(
            RT_SIGACTION,
            signum,
            (&sigaction) as *const abi::SigAction,
            0,
            size_of::<abi::SigSet>()
        )
    } {
        0 => Ok(()),
        other => Err(Errno(other as i32)),
    }
}

pub fn fcntl(fd: &SysFd, op: usize, arg: usize) -> Result<isize, Errno> {
    match unsafe { syscall!(FCNTL, fd.0, op, arg) } as isize {
        result if result >= 0 => Ok(result),
        other => Err(Errno(other as i32)),
    }
}

#[allow(dead_code)]
pub fn lseek(fd: &SysFd, pos: usize, whence: isize) -> Result<usize, Errno> {
    let result = unsafe { syscall!(LSEEK, fd.0, pos, whence) as isize };
    if result >= 0 {
        Ok(result as usize)
    } else {
        Err(Errno(result as i32))
    }
}

pub fn getdents(fd: &SysFd, buffer: &mut [u8]) -> Result<usize, Errno> {
    let result = unsafe { syscall!(GETDENTS64, fd.0, buffer.as_mut_ptr(), buffer.len()) as isize };
    if result >= 0 {
        Ok(result as usize)
    } else {
        Err(Errno(result as i32))
    }
}

pub fn pread(fd: &SysFd, bytes: &mut [u8], offset: usize) -> Result<usize, Errno> {
    let result =
        unsafe { syscall!(PREAD64, fd.0, bytes.as_mut_ptr(), bytes.len(), offset) as isize };
    if result >= 0 {
        Ok(result as usize)
    } else {
        Err(Errno(result as i32))
    }
}

pub fn pread_exact(fd: &SysFd, bytes: &mut [u8], offset: usize) -> Result<(), Errno> {
    match pread(fd, bytes, offset) {
        Ok(len) if len == bytes.len() => Ok(()),
        Ok(_) => Err(Errno(-abi::EIO)),
        Err(e) => Err(e),
    }
}

pub fn getrandom(bytes: &mut [u8], flags: isize) -> Result<usize, Errno> {
    let result = unsafe { syscall!(GETRANDOM, bytes.as_mut_ptr(), bytes.len(), flags) as isize };
    if result >= 0 {
        Ok(result as usize)
    } else {
        Err(Errno(result as i32))
    }
}

pub fn getrandom_usize() -> usize {
    let mut bytes = 0usize.to_ne_bytes();
    getrandom(&mut bytes, 0).expect("getrandom");
    usize::from_ne_bytes(bytes)
}

pub fn open_self_fd() -> Result<SysFd, Errno> {
    let flags = abi::O_RDONLY | abi::O_CLOEXEC | abi::O_DIRECTORY;
    unsafe { open(&PROC_SELF_FD, flags, 0) }
}

pub fn open_self_exe() -> Result<SysFd, Errno> {
    let flags = abi::O_RDONLY | abi::O_CLOEXEC;
    unsafe { open(&PROC_SELF_EXE, flags, 0) }
}

pub struct DirIterator<'f, S: ArrayLength<u8>, F: Fn(Dirent<'_>) -> V, V> {
    dir_fd: &'f SysFd,
    buf: Vec<u8, S>,
    buf_position: usize,
    callback: F,
}

impl<'f, S: ArrayLength<u8>, F: Fn(Dirent<'_>) -> V, V> DirIterator<'f, S, F, V> {
    pub fn new(dir_fd: &'f SysFd, callback: F) -> Self {
        DirIterator {
            dir_fd,
            callback,
            buf: Vec::new(),
            buf_position: 0,
        }
    }
}

#[derive(Debug)]
pub struct Dirent<'a> {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_type: u8,
    pub d_name: &'a [u8],
}

unsafe impl Plain for LinuxDirentHeader {}

impl<'f, S: ArrayLength<u8>, F: Fn(Dirent<'_>) -> V, V> Iterator for DirIterator<'f, S, F, V> {
    type Item = Result<V, Errno>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf_position == self.buf.len() {
            unsafe {
                let buffer = slice::from_raw_parts_mut(self.buf.as_mut_ptr(), self.buf.capacity());
                match getdents(&self.dir_fd, buffer) {
                    Err(err) => return Some(Err(err)),
                    Ok(len) => {
                        assert!(len <= self.buf.capacity());
                        self.buf.set_len(len);
                        self.buf_position = 0;
                    }
                }
            }
        }
        if self.buf_position == self.buf.len() {
            None
        } else {
            let mut header: LinuxDirentHeader = unsafe { mem::zeroed() };
            let raw_bytes = &self.buf[self.buf_position..];
            plain::copy_from_bytes(&mut header, raw_bytes).expect("dirent header");
            let reclen = header.d_reclen as usize;
            assert!(reclen >= size_of::<LinuxDirentHeader>());
            assert!(raw_bytes.len() >= reclen);
            self.buf_position += reclen;
            let d_name = &raw_bytes[offset_of!(LinuxDirentHeader, d_name)..];
            let d_name = d_name
                .split(|b| *b == 0)
                .next()
                .expect("dirent nul termination");
            let dirent = Dirent {
                d_ino: header.d_ino,
                d_off: header.d_off,
                d_type: header.d_type,
                d_name,
            };
            let value = (self.callback)(dirent);
            Some(Ok(value))
        }
    }
}

#[cfg(test)]
mod test {
    use super::{lseek, open_self_fd, DirIterator, Dirent};
    use crate::abi;
    use std::{
        collections::HashSet,
        fs::File,
        os::unix::io::{FromRawFd, IntoRawFd, RawFd},
        string::String,
    };
    use typenum::*;

    #[test]
    fn self_fd_iter() {
        let dir_fd = open_self_fd().unwrap();

        let mut fds_created: HashSet<RawFd> = HashSet::new();
        let mut fds_seen: HashSet<RawFd> = HashSet::new();
        let mut fds_closed: HashSet<RawFd> = HashSet::new();

        file_limit::set_to_max().unwrap();
        for _ in 0..3000 {
            fds_created.insert(File::open("/dev/null").unwrap().into_raw_fd());
        }

        fn callback(dirent: Dirent<'_>) -> (u8, String) {
            (
                dirent.d_type,
                String::from_utf8(dirent.d_name.to_vec()).unwrap(),
            )
        }

        // testing small buffers specifically; 64 bytes only holds about 2 dirents
        let mut iter = DirIterator::<U64, _, _>::new(&dir_fd, callback);

        for result in &mut iter {
            let (d_type, d_name) = result.unwrap();
            if d_type == abi::DT_DIR {
                assert!(d_name == "." || d_name == "..");
            } else {
                assert_eq!(d_type, abi::DT_LNK);
                let fd: RawFd = d_name.parse().unwrap();
                assert!(fds_seen.insert(fd));
                if fds_created.contains(&fd) {
                    unsafe { File::from_raw_fd(fd) };
                    assert!(fds_closed.insert(fd));
                }
            }
        }

        assert!(iter.next().is_none());
        assert!(fds_seen.is_superset(&fds_created));
        assert_eq!(fds_created, fds_closed);

        // seek and re-read, with the same fd
        lseek(&dir_fd, 0, abi::SEEK_SET).unwrap();
        let iter = DirIterator::<U64, _, _>::new(&dir_fd, callback);
        for result in iter {
            let (d_type, d_name) = result.unwrap();
            if d_type == abi::DT_DIR {
                assert!(d_name == "." || d_name == "..");
            } else {
                assert_eq!(d_type, abi::DT_LNK);
                let fd: RawFd = d_name.parse().unwrap();
                assert!(fds_seen.contains(&fd));
                assert!(!fds_closed.contains(&fd));
            }
        }

        dir_fd.close().unwrap();
    }
}
