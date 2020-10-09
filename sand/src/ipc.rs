use core::ptr;
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
        Socket {
            fd: fd.clone()
        }
    }

    extern fn handle_sigio(num: u32) {
    	assert_eq!(num, abi::SIGIO);
        println!("sigio");
    }

    pub fn recv(&self) -> Option<MessageToSand> {
        let mut buffer = [0 as u8; BUFFER_SIZE];
        let mut iov = abi::IOVec {
            base: &mut buffer[0] as *mut u8,
            len: BUFFER_SIZE,
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
        let result = unsafe { syscall!(RECVMSG, self.fd.0, &msghdr as *const abi::MsgHdr, flags) as isize };
        match result {
            len if len > 0 => match deserialize(&buffer) {
                Ok((message, bytes_used)) if bytes_used == len as usize => Some(message),
                other => panic!("recvmsg deserialized {} bytes to unexpected value, {:?}", len, other),
            },
            EAGAIN => None,
            other => panic!("recvmsg ({})", other),
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
        let result = unsafe { syscall!(SENDMSG, self.fd.0, &msghdr as *const abi::MsgHdr, flags) as isize };
        assert_eq!(result as isize, len as isize);
    }
}
