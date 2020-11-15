use crate::{abi, protocol::SysFd};
use core::{fmt, slice, str};
use sc::syscall;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (
        $crate::nolibc::write_stderr(core::format_args!( $($arg)* ))
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
    pub fn close(&self) -> Result<(), ()> {
        if 0 == unsafe { syscall!(CLOSE, self.0) } {
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn read_exact(&self, bytes: &mut [u8]) -> Result<(), isize> {
        let mut offset = 0;
        while offset < bytes.len() {
            let slice = &mut bytes[offset..];
            let result = unsafe {
                syscall!(READ, self.0, slice.as_mut_ptr() as usize, slice.len()) as isize
            };
            if result <= 0 {
                return Err(result);
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

pub fn socketpair(domain: usize, type_: usize, protocol: usize) -> Result<(SysFd, SysFd), isize> {
    let mut pair = [0u32; 2];
    let result =
        unsafe { syscall!(SOCKETPAIR, domain, type_, protocol, pair.as_mut_ptr()) as isize };
    if result == 0 {
        Ok((SysFd(pair[0]), SysFd(pair[1])))
    } else {
        Err(result)
    }
}

pub fn write_stderr(msg: fmt::Arguments) {
    let mut stderr = SysFd(2);
    if fmt::write(&mut stderr, msg).is_err() {
        exit(EXIT_IO_ERROR);
    }
}

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

pub unsafe fn c_str_slice(s: *const u8) -> &'static [u8] {
    slice::from_raw_parts(s, 1 + c_strlen(s))
}

pub unsafe fn c_strv_len(strv: *const *const u8) -> usize {
    let mut count: usize = 0;
    while !(*strv.add(count)).is_null() {
        count += 1;
    }
    count
}

pub unsafe fn c_strv_slice(strv: *const *const u8) -> &'static [*const u8] {
    slice::from_raw_parts(strv, c_strv_len(strv))
}

#[naked]
unsafe extern "C" fn sigreturn() {
    syscall!(RT_SIGRETURN);
    unreachable!();
}

pub fn signal(signum: u32, handler: extern "C" fn(u32)) -> Result<(), isize> {
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
            core::mem::size_of::<abi::SigSet>()
        )
    } {
        0 => Ok(()),
        other => Err(other as isize),
    }
}

pub fn fcntl(fd: &SysFd, op: usize, arg: usize) -> Result<(), isize> {
    match unsafe { syscall!(FCNTL, fd.0, op, arg) } {
        0 => Ok(()),
        other => Err(other as isize),
    }
}
