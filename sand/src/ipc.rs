use crate::{
    abi,
    abi::{CMsgHdr, IOVec, MsgHdr},
    nolibc::{fcntl, getpid, signal},
    protocol::{
        buffer::{FilesMax, IPCBuffer},
        MessageFromSand, MessageToSand, SysFd,
    },
};
use core::{
    mem::size_of,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use sc::syscall;
use typenum::Unsigned;

static SIGIO_FLAG: AtomicBool = AtomicBool::new(true);

pub struct Socket {
    fd: SysFd,
    recv_buffer: IPCBuffer,
}

#[derive(Default)]
#[repr(C)]
struct CMsgBuffer {
    hdr: CMsgHdr,
    files: [u32; FilesMax::USIZE],
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
        let mut iov = IOVec {
            base: self.recv_buffer.as_slice_mut().bytes.as_mut_ptr(),
            len: self.recv_buffer.byte_capacity(),
        };
        let mut cmsg: CMsgBuffer = Default::default();
        let mut msghdr = MsgHdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov as *mut IOVec,
            msg_iovlen: 1,
            msg_control: &mut cmsg as *mut CMsgBuffer as *mut usize,
            msg_controllen: size_of::<CMsgBuffer>(),
            msg_flags: 0,
        };
        let flags = abi::MSG_DONTWAIT;
        let result = unsafe { syscall!(RECVMSG, self.fd.0, &mut msghdr as *mut MsgHdr, flags) };
        match result as isize {
            len if len > 0 => {
                let num_files = if cmsg.hdr.cmsg_len == 0 {
                    0
                } else {
                    assert!(cmsg.hdr.cmsg_len >= size_of::<CMsgHdr>());
                    let data_len = cmsg.hdr.cmsg_len - size_of::<CMsgHdr>();
                    assert_eq!(cmsg.hdr.cmsg_level, abi::SOL_SOCKET);
                    assert_eq!(cmsg.hdr.cmsg_type, abi::SCM_RIGHTS);
                    assert!(data_len % size_of::<u32>() == 0);
                    data_len / size_of::<u32>()
                };
                unsafe { self.recv_buffer.set_len(len as usize, num_files) };
                for idx in 0..num_files {
                    self.recv_buffer.as_slice_mut().files[idx] = SysFd(cmsg.files[idx]);
                }
            }
            e if e == -abi::EAGAIN as isize => (),
            e if e == 0 || e == -abi::ECONNRESET as isize => panic!("disconnected from ipc server"),
            e => panic!("ipc recvmsg error, ({})", e),
        }
    }

    pub fn send(&self, message: &MessageFromSand) {
        let mut buffer = IPCBuffer::new();
        buffer.push_back(message).expect("serialize failed");
        let mut cmsg = CMsgBuffer {
            hdr: CMsgHdr {
                cmsg_len: size_of::<CMsgHdr>() + size_of::<u32>() * buffer.as_slice().files.len(),
                cmsg_level: abi::SOL_SOCKET,
                cmsg_type: abi::SCM_RIGHTS,
            },
            files: Default::default(),
        };
        for (idx, file) in buffer.as_slice().files.iter().enumerate() {
            cmsg.files[idx] = file.0;
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
            msg_control: &mut cmsg as *mut CMsgBuffer as *mut usize,
            msg_controllen: cmsg.hdr.cmsg_len,
            msg_flags: 0,
        };
        let flags = abi::MSG_DONTWAIT;
        let result =
            unsafe { syscall!(SENDMSG, self.fd.0, &msghdr as *const abi::MsgHdr, flags) as isize };
        assert_eq!(result as usize, buffer.as_slice().bytes.len());
    }
}
