use core::ptr;
use sc::syscall;
use crate::abi;
use crate::nolibc::{self, SysFd};
use crate::protocol::{MessageFromSand, MessageToSand, BUFFER_SIZE, serialize, deserialize};

#[derive(Debug)]
pub struct Socket {
    fd: SysFd,
    recv_buffer: [u8; BUFFER_SIZE],
    recv_begin: usize,
    recv_end: usize,
}

impl Socket {
    pub fn from_sys_fd(fd: &SysFd) -> Socket {
        nolibc::signal(abi::SIGIO, Socket::handle_sigio).expect("setting up sigio handler");
        nolibc::fcntl_setfl(fd, abi::FASYNC | abi::O_NONBLOCK).expect("setting socket flags");
        Socket {
            fd: fd.clone(),
            recv_buffer: [0; BUFFER_SIZE],
            recv_begin: 0,
            recv_end: 0,
        }
    }

    extern fn handle_sigio(num: u32) {
    	assert_eq!(num, abi::SIGIO);
        println!("sigio");
    }

    pub fn recv(&mut self) -> Option<MessageToSand> {
        if self.recv_begin == self.recv_end {
            self.fill_recv_buffer();
            println!("filled {} {}", self.recv_begin, self.recv_end);
        }
        if self.recv_begin == self.recv_end {
            None
        } else {
            println!("try deserialize {} {}", self.recv_begin, self.recv_end); 
            match deserialize(&self.recv_buffer[self.recv_begin .. self.recv_end]) {
                Ok((message, bytes_used)) => {
                    self.recv_begin += bytes_used;
                    assert!(self.recv_begin <= self.recv_end);
                    Some(message)
                },
                other => panic!("recvmsg deserialized to unexpected value, {:?}", other),
            }
        }
    }

    fn fill_recv_buffer(&mut self) {
        let mut iov = abi::IOVec {
            base: &mut self.recv_buffer[0] as *mut u8,
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
        println!("recvmsg {}", result);
        self.recv_begin = 0;
        self.recv_end = match result {
            EAGAIN => 0,
            len if len > 0 => len as usize,
            other => panic!("recvmsg ({})", other),
        };
        println!("filled_ {} {}", self.recv_begin, self.recv_end);
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
