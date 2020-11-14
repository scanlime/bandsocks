use crate::{
    abi,
    protocol::{Errno, SysFd},
};
use sc::syscall;

#[derive(Debug)]
pub struct Header {
    buffer: [u8; abi::BINPRM_BUF_SIZE],
}

impl Header {
    pub async fn load(sys_fd: &SysFd) -> Result<Header, Errno> {
        let mut buffer = [0u8; abi::BINPRM_BUF_SIZE];
        let result =
            unsafe { syscall!(PREAD64, sys_fd.0, buffer.as_mut_ptr(), buffer.len(), 0) as isize };
        if result >= 0 {
            Ok(Header { buffer })
        } else {
            Err(Errno(result as i32))
        }
    }
}
