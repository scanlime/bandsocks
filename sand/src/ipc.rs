use crate::{
    abi,
    abi::{CMsgHdr, IOVec, MsgHdr},
    nolibc::{exit, getpid, signal, File},
    protocol::{
        buffer,
        buffer::{FilesMax, IPCBuffer},
        MessageFromSand, MessageToSand, SysFd,
    },
    EXIT_DISCONNECTED,
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
    file: File,
    recv_buffer: IPCBuffer,
}

#[repr(C)]
struct CMsgBuffer {
    hdr: CMsgHdr,
    files: [u32; FilesMax::USIZE],
}

impl Socket {
    pub fn new(file: File) -> Socket {
        Socket::setup_sigio(&file);
        Socket {
            file,
            recv_buffer: IPCBuffer::new(),
        }
    }

    fn setup_sigio(file: &File) {
        // Note that we want blocking writes and non-blocking reads. See the flags in
        // sendmsg/recvmsg.
        signal(abi::SIGIO, Socket::handle_sigio).expect("setting up sigio handler");
        file.fcntl(abi::F_SETOWN, getpid())
            .expect("setting socket owner");
        file.fcntl(abi::F_SETFL, abi::FASYNC)
            .expect("setting socket flags");
    }

    extern "C" fn handle_sigio(num: u32) {
        assert_eq!(num, abi::SIGIO as u32);
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
                Err(buffer::Error::UnexpectedEnd) => None,
                Err(e) => panic!("deserialize failed, {:x?}", e),
            }
        }
    }

    fn recv_to_buffer(&mut self) {
        let available = self.recv_buffer.begin_fill();
        let mut iov = IOVec {
            base: available.bytes.as_mut_ptr(),
            len: available.bytes.len(),
        };
        let mut cmsg: CMsgBuffer = unsafe { core::mem::zeroed() };
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
        let result =
            unsafe { syscall!(RECVMSG, self.file.fd.0, &mut msghdr as *mut MsgHdr, flags) };
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
                for idx in 0..num_files {
                    available.files[idx] = SysFd(cmsg.files[idx]);
                }
                self.recv_buffer.commit_fill(len as usize, num_files);
            }
            e if e == -abi::EAGAIN as isize => (),
            e if e == 0 || e == -abi::ECONNRESET as isize => exit(EXIT_DISCONNECTED),
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
            files: unsafe { core::mem::zeroed() },
        };
        let slice = buffer.as_slice();
        for (idx, file) in slice.files.iter().enumerate() {
            cmsg.files[idx] = file.0;
        }
        let mut iov = IOVec {
            base: slice.bytes.as_ptr() as *mut u8,
            len: slice.bytes.len(),
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
        let flags = 0;
        let result = unsafe {
            syscall!(
                SENDMSG,
                self.file.fd.0,
                &msghdr as *const abi::MsgHdr,
                flags
            ) as isize
        };
        assert_eq!(result, buffer.as_slice().bytes.len() as isize);
    }
}
