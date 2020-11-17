use crate::{
    abi,
    abi::{CMsgHdr, IOVec, MsgHdr},
    protocol::{Errno, SysFd, VPtr},
    remote::{
        mem::{fault_or, read_value, write_padded_bytes, write_padded_value},
        trampoline::Trampoline,
        RemoteFd,
    },
};
use core::{mem::size_of, pin::Pin, ptr};
use sc::{nr, syscall};

#[derive(Debug)]
pub struct CantDropScratchpad;

impl Drop for CantDropScratchpad {
    fn drop(&mut self) {
        panic!("leaking scratchpad")
    }
}

#[derive(Debug)]
pub struct Scratchpad<'q, 's, 't, 'r> {
    pub trampoline: &'r mut Trampoline<'q, 's, 't>,
    pub page_ptr: VPtr,
    guard: CantDropScratchpad,
}

impl<'q, 's, 't, 'r> Scratchpad<'q, 's, 't, 'r> {
    pub async fn new(
        trampoline: &'r mut Trampoline<'q, 's, 't>,
    ) -> Result<Scratchpad<'q, 's, 't, 'r>, Errno> {
        let page_ptr = trampoline
            .mmap(
                VPtr(0),
                abi::PAGE_SIZE,
                abi::PROT_READ | abi::PROT_WRITE,
                abi::MAP_PRIVATE | abi::MAP_ANONYMOUS,
                &RemoteFd(0),
                0,
            )
            .await?;
        Ok(Scratchpad {
            trampoline,
            page_ptr,
            guard: CantDropScratchpad,
        })
    }

    pub async fn free(self) -> Result<(), Errno> {
        core::mem::forget(self.guard);
        self.trampoline.munmap(self.page_ptr, abi::PAGE_SIZE).await
    }

    #[allow(dead_code)]
    pub async fn debug_loop(&mut self) -> ! {
        loop {
            self.write_fd(&RemoteFd(1), b"debug loop\n").await.unwrap();
            self.sleep(&abi::TimeSpec::from_secs(10)).await.unwrap();
        }
    }

    pub async fn write_fd(&mut self, fd: &RemoteFd, bytes: &[u8]) -> Result<usize, Errno> {
        if bytes.len() > abi::PAGE_SIZE - size_of::<usize>() {
            return Err(Errno(-abi::EINVAL));
        }
        fault_or(write_padded_bytes(
            self.trampoline.stopped_task,
            self.page_ptr,
            bytes,
        ))?;
        let result = self
            .trampoline
            .syscall(
                nr::WRITE,
                &[
                    fd.0 as isize,
                    self.page_ptr.0 as isize,
                    bytes.len() as isize,
                ],
            )
            .await;
        if result > 0 {
            Ok(result as usize)
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn sleep(&mut self, duration: &abi::TimeSpec) -> Result<(), Errno> {
        fault_or(unsafe {
            write_padded_value(self.trampoline.stopped_task, self.page_ptr, duration)
        })?;
        let result = self
            .trampoline
            .syscall(nr::NANOSLEEP, &[self.page_ptr.0 as isize, 0])
            .await;
        if result == 0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn memfd_create(&mut self, name: &[u8], flags: isize) -> Result<RemoteFd, Errno> {
        if name.len() > abi::PAGE_SIZE - size_of::<usize>() {
            return Err(Errno(-abi::EINVAL));
        }
        fault_or(write_padded_bytes(
            self.trampoline.stopped_task,
            self.page_ptr,
            name,
        ))?;
        let result = self
            .trampoline
            .syscall(
                nr::MEMFD_CREATE,
                &[self.page_ptr.0 as isize, flags as isize],
            )
            .await;
        if result > 0 {
            Ok(RemoteFd(result as u32))
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn send_fd(&mut self, local: &SysFd) -> Result<RemoteFd, Errno> {
        let socket_pair = &self.trampoline.stopped_task.task.task_data.socket_pair;
        let local_socket_fd = socket_pair.tracer.0;
        let remote_socket_fd = socket_pair.remote.0;

        #[derive(Debug, Clone)]
        #[repr(C)]
        struct CMsg {
            hdr: CMsgHdr,
            fd: u32,
        }

        #[derive(Debug, Clone)]
        #[repr(C)]
        struct Layout {
            hdr: MsgHdr,
            cmsg: CMsg,
            iov: IOVec,
            msg: [u8; 1],
        };

        const BASE_LAYOUT: Layout = Layout {
            hdr: MsgHdr {
                msg_name: ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: ptr::null_mut(),
                msg_iovlen: 1,
                msg_control: ptr::null_mut(),
                msg_controllen: size_of::<CMsg>(),
                msg_flags: 0,
            },
            cmsg: CMsg {
                hdr: CMsgHdr {
                    cmsg_len: size_of::<CMsg>(),
                    cmsg_level: abi::SOL_SOCKET,
                    cmsg_type: abi::SCM_RIGHTS,
                },
                fd: -1i32 as u32,
            },
            iov: IOVec {
                base: ptr::null_mut(),
                len: 1,
            },
            msg: [0],
        };

        let mut local_layout = BASE_LAYOUT.clone();
        let mut remote_layout = BASE_LAYOUT.clone();

        local_layout.cmsg.fd = local.0;
        let mut local_pin = Pin::new(&mut local_layout);
        let local_ptr = &local_pin.as_mut().get_mut().hdr as *const MsgHdr;
        local_pin.as_mut().get_mut().hdr.msg_iov =
            &mut local_pin.as_mut().get_mut().iov as *mut IOVec;
        local_pin.as_mut().get_mut().hdr.msg_control =
            &mut local_pin.as_mut().get_mut().cmsg as *mut CMsg as *mut usize;
        local_pin.as_mut().get_mut().iov.base = local_pin.as_mut().get_mut().msg.as_mut_ptr();

        remote_layout.hdr.msg_iov = self.page_ptr.add(offset_of!(Layout, iov)).0 as *mut IOVec;
        remote_layout.hdr.msg_control = self.page_ptr.add(offset_of!(Layout, cmsg)).0 as *mut usize;
        remote_layout.iov.base = self.page_ptr.add(offset_of!(Layout, msg)).0 as *mut u8;

        fault_or(unsafe {
            write_padded_value(self.trampoline.stopped_task, self.page_ptr, &remote_layout)
        })?;

        let local_flags = abi::MSG_DONTWAIT;
        let remote_flags = 0;

        let local_result =
            unsafe { syscall!(SENDMSG, local_socket_fd, local_ptr, local_flags) as isize };
        if local_result != 1 {
            return Err(Errno(local_result as i32));
        }

        let remote_result = self
            .trampoline
            .syscall(
                nr::RECVMSG,
                &[
                    remote_socket_fd as isize,
                    self.page_ptr.0 as isize,
                    remote_flags,
                ],
            )
            .await;
        if remote_result != 1 {
            return Err(Errno(remote_result as i32));
        }

        let remote_cmsg: CMsg = fault_or(unsafe {
            read_value(
                self.trampoline.stopped_task,
                self.page_ptr.add(offset_of!(Layout, cmsg)),
            )
        })?;
        if remote_cmsg.hdr == BASE_LAYOUT.cmsg.hdr {
            Ok(RemoteFd(remote_cmsg.fd))
        } else {
            Err(Errno(-abi::EIO))
        }
    }
}
