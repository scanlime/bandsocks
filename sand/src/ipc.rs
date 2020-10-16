use crate::{
    abi,
    abi::CMsgRights,
    nolibc::{fcntl, getpid, signal},
    protocol::{deserialize, serialize, SysFd, MessageFromSand, MessageToSand},
};
use as_slice::AsMutSlice;
use core::{
    mem::size_of,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use heapless::Vec;
use sc::syscall;
use typenum::*;

static SIGIO_FLAG: AtomicBool = AtomicBool::new(false);
type BufferedBytesMax = U256;
type BufferedFilesMax = U16;

#[derive(Debug)]
pub struct Socket {
    fd: SysFd,
    recv_byte_buffer: Vec<u8, BufferedBytesMax>,
    recv_byte_offset: usize,
    recv_file_buffer: Vec<CMsgRights, BufferedFilesMax>,
    recv_file_offset: usize,
}

impl Socket {
    pub fn from_sys_fd(fd: &SysFd) -> Socket {
        Socket::setup_sigio(fd);
        Socket {
            fd: fd.clone(),
            recv_byte_buffer: Vec::new(),
            recv_byte_offset: 0,
            recv_file_buffer: Vec::new(),
            recv_file_offset: 0,
        }
    }

    fn setup_sigio(fd: &SysFd) {
        signal(abi::SIGIO, Socket::handle_sigio).expect("setting up sigio handler");
        fcntl(fd, abi::F_SETFL, abi::FASYNC | abi::O_NONBLOCK).expect("setting socket flags");
        fcntl(fd, abi::F_SETOWN, getpid()).expect("setting socket owner");
    }

    extern "C" fn handle_sigio(num: u32) {
        assert_eq!(num, abi::SIGIO);
        SIGIO_FLAG.store(true, Ordering::SeqCst);
    }

    pub fn recv(&mut self) -> Option<MessageToSand> {
        if self.recv_byte_offset == self.recv_byte_buffer.len() {
            if SIGIO_FLAG.swap(false, Ordering::SeqCst) {
                self.recv_to_buffer();
            }
        }
        if self.recv_byte_offset == self.recv_byte_buffer.len() {
            None
        } else {
            match deserialize(&self.recv_byte_buffer[self.recv_byte_offset..]) {
                Ok((message, bytes_used)) => {
                    self.recv_byte_offset += bytes_used;
                    assert!(self.recv_byte_offset <= self.recv_byte_buffer.len());
                    Some(message)
                }
                other => panic!("deserialize failed, {:x?}", other),
            }
        }
    }

    pub fn recv_file(&mut self) -> Option<SysFd> {
        if self.recv_file_offset == self.recv_file_buffer.len() {
            None
        } else {
            let rights = &self.recv_file_buffer[self.recv_file_offset];
            self.recv_file_offset += 1;
            assert_eq!(rights.hdr.cmsg_len, size_of::<CMsgRights>());
            assert_eq!(rights.hdr.cmsg_level, abi::SOL_SOCKET);
            assert_eq!(rights.hdr.cmsg_type, abi::SCM_RIGHTS);
            assert!(rights.fd > 0);
            Some(SysFd(rights.fd as u32))
        }
    }

    fn recv_to_buffer(&mut self) {
        assert_eq!(self.recv_byte_offset, self.recv_byte_buffer.len());
        assert_eq!(self.recv_file_offset, self.recv_file_buffer.len());
        self.recv_byte_offset = 0;
        self.recv_byte_buffer.clear();
        self.recv_file_offset = 0;
        self.recv_file_buffer.clear();

        let mut iov = abi::IOVec {
            base: self.recv_byte_buffer.as_mut_slice().as_mut_ptr(),
            len: self.recv_byte_buffer.capacity(),
        };
        let mut msghdr = abi::MsgHdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov as *mut abi::IOVec,
            msg_iovlen: 1,
            msg_control: self.recv_file_buffer.as_mut_slice().as_mut_ptr() as *mut usize,
            msg_controllen: self.recv_file_buffer.capacity() * size_of::<CMsgRights>(),
            msg_flags: 0,
        };
        let flags = abi::MSG_DONTWAIT;
        unsafe {
            match syscall!(RECVMSG, self.fd.0, &mut msghdr as *mut abi::MsgHdr, flags) as isize {
                len if len > 0 => {
                    self.recv_byte_buffer.set_len(len as usize);
                    self.recv_file_buffer
                        .set_len(msghdr.msg_controllen as usize / size_of::<CMsgRights>());
                }
                err if err == -abi::EAGAIN => (),
                err if err == 0 => panic!("disconnected from ipc server"),
                err => panic!("recvmsg ({})", err),
            }
        }
    }

    pub fn send(&self, message: &MessageFromSand) {
        let mut buffer = [0; BufferedBytesMax::USIZE];
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
        let result =
            unsafe { syscall!(SENDMSG, self.fd.0, &msghdr as *const abi::MsgHdr, flags) as isize };
        assert_eq!(result as isize, len as isize);
    }
}
