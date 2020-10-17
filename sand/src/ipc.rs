use crate::{
    abi,
    abi::{CMsgHdr, CMsgRights, IOVec, MsgHdr},
    nolibc::{fcntl, getpid, signal},
    protocol::{
        buffer::{FilesMax, IPCBuffer},
        MessageFromSand, MessageToSand, SysFd,
    },
};
use as_slice::AsMutSlice;
use core::{
    mem::size_of,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use heapless::Vec;
use sc::syscall;

static SIGIO_FLAG: AtomicBool = AtomicBool::new(false);

pub struct Socket {
    fd: SysFd,
    recv_buffer: IPCBuffer,
}

impl Socket {
    pub fn from_sys_fd(fd: &SysFd) -> Socket {
        Socket::setup_sigio(fd);
        Socket {
            fd: fd.clone(),
            recv_buffer: IPCBuffer::new(),
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
        if self.recv_buffer.is_empty() && SIGIO_FLAG.swap(false, Ordering::SeqCst) {
            self.recv_to_buffer();
        }
        if self.recv_buffer.is_empty() {
            None
        } else {
            match self.recv_buffer.pop_front() {
                Ok(message) => Some(message),
                Err(e) => panic!("deserialize failed, {:x?}", e),
            }
        }
    }

    fn recv_to_buffer(&mut self) {
        assert!(self.recv_buffer.is_empty());
        self.recv_buffer.reset();
        let mut cmsg_buffer: Vec<CMsgRights, FilesMax> = Vec::new();
        let mut iov = IOVec {
            base: self.recv_buffer.as_slice_mut().bytes.as_mut_ptr(),
            len: self.recv_buffer.byte_capacity(),
        };
        let mut msghdr = MsgHdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov as *mut IOVec,
            msg_iovlen: 1,
            msg_control: cmsg_buffer.as_mut_slice().as_mut_ptr() as *mut usize,
            msg_controllen: cmsg_buffer.capacity() * size_of::<CMsgRights>(),
            msg_flags: 0,
        };
        let flags = abi::MSG_DONTWAIT;
        let result = unsafe { syscall!(RECVMSG, self.fd.0, &mut msghdr as *mut MsgHdr, flags) };
        match result as isize {
            len if len > 0 => {
                let num_files = msghdr.msg_controllen as usize / size_of::<CMsgRights>();
                unsafe {
                    cmsg_buffer.set_len(num_files);
                    self.recv_buffer.set_len(len as usize, num_files);
                }
                for file in 0..num_files {
                    println!(">{}", file);
                    let rights = &cmsg_buffer[file];
                    assert_eq!(rights.hdr.cmsg_len, size_of::<CMsgRights>());
                    assert_eq!(rights.hdr.cmsg_level, abi::SOL_SOCKET);
                    assert_eq!(rights.hdr.cmsg_type, abi::SCM_RIGHTS);
                    assert!(rights.fd > 0);
                    self.recv_buffer.as_slice_mut().files[file] = SysFd(rights.fd as u32);
                    println!("<{}", file);
                }
            }
            e if e == -abi::EAGAIN => (),
            e if e == 0 || e == -abi::ECONNRESET => panic!("disconnected from ipc server"),
            e => panic!("ipc recvmsg error, ({})", e),
        }
        println!("did recv_to_buffer");
    }

    pub fn send(&self, message: &MessageFromSand) {
        let mut buffer = IPCBuffer::new();
        buffer.push_back(message).expect("serialize failed");
        let mut cmsg_buffer: Vec<CMsgRights, FilesMax> = Vec::new();
        for file in buffer.as_slice().files {
            cmsg_buffer
                .push(CMsgRights {
                    fd: file.0 as i32,
                    hdr: CMsgHdr {
                        cmsg_len: size_of::<CMsgRights>(),
                        cmsg_level: abi::SOL_SOCKET,
                        cmsg_type: abi::SCM_RIGHTS,
                    },
                })
                .unwrap();
        }
        let mut iov = IOVec {
            base: buffer.as_slice_mut().bytes.as_mut_ptr(),
            len: buffer.as_slice().bytes.len(),
        };
        let msghdr = MsgHdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov as *mut IOVec,
            msg_iovlen: 1,
            msg_control: cmsg_buffer.as_mut_slice().as_mut_ptr() as *mut usize,
            msg_controllen: cmsg_buffer.len() * size_of::<CMsgRights>(),
            msg_flags: 0,
        };
        let flags = abi::MSG_DONTWAIT;
        let result =
            unsafe { syscall!(SENDMSG, self.fd.0, &msghdr as *const abi::MsgHdr, flags) as isize };
        assert_eq!(result as usize, buffer.as_slice().bytes.len());
    }
}
