// This code may not be used for any purpose. Be gay, do crime.

use core::ptr;
use sc::syscall;
use crate::abi;
use crate::nolibc::SysFd;
use crate::protocol::{MessageFromSand, MessageToSand, BUFFER_SIZE, serialize, deserialize};

pub struct Socket {
    fd: SysFd
}

impl Socket {
    pub fn from_sys_fd(fd: SysFd) -> Socket {
        Socket {
            fd
        }
    }

    pub fn send(&self, message: &MessageFromSand) {
        let mut buffer = [0; BUFFER_SIZE];
        let len = serialize(&mut buffer, message).unwrap();
        let mut iov = abi::IOVec {
            base: &mut buffer[0] as *mut u8,
            len,
        };
        let msghdr = abi::MsgHdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov as *mut abi::IOVec,
            msg_iovlen: 1,
            msg_control: ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };
        let flags = abi::MSG_DONTWAIT;
        let result = unsafe { syscall!(SENDMSG, self.fd.0, &msghdr as *const abi::MsgHdr, flags)};
        assert_eq!(result as isize, len as isize);
    }
}
