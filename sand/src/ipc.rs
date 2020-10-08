use core::ptr;
use core::mem;
use sc::syscall;
use crate::abi;
use crate::nolibc::{self, SysFd};
use crate::protocol::{MessageFromSand, MessageToSand, BUFFER_SIZE, serialize, deserialize};

#[derive(Debug)]
pub struct Socket {
    fd: SysFd
}

impl Socket {
    pub fn from_sys_fd(fd: &SysFd) -> Socket {
        nolibc::signal(abi::SIGIO, Socket::handle_sigio).expect("setting up sigio handler");
        nolibc::fcntl_setfl(fd, abi::FASYNC | abi::O_NONBLOCK).expect("setting socket flags");
        unsafe { syscall!(KILL, syscall!(GETPID), abi::SIGIO)  };
        Socket {
            fd: fd.clone()
        }
    }

    extern fn handle_sigio(num: u32) {
        println!("signal! {}", num);
        nolibc::sigreturn();
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
