use crate::{
    abi,
    abi::{CMsgHdr, IOVec, MsgHdr, SyscallInfo},
    process::{
        maps::{MapsIterator, MemArea, MemAreaName},
        task::StoppedTask,
        Event,
    },
    protocol::{Errno, SysFd, VPtr},
    ptrace,
};
use core::{
    mem::{size_of, MaybeUninit},
    pin::Pin,
    ptr,
};
use sc::{nr, syscall};

#[derive(Debug, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct RemoteFd(pub u32);

#[derive(Debug)]
pub struct Trampoline<'q, 's, 't> {
    pub stopped_task: &'t mut StoppedTask<'q, 's>,
    pub vdso: MemArea,
    pub vvar: MemArea,
    pub vsyscall: Option<MemArea>,
    pub vdso_syscall: VPtr,
}

fn find_syscall<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    vdso: &MemArea,
) -> Result<VPtr, ()> {
    const X86_64_SYSCALL: [u8; 2] = [0x0f, 0x05];
    mem_find_bytes(
        stopped_task,
        VPtr(vdso.start),
        vdso.end - vdso.start,
        &X86_64_SYSCALL,
    )
}

impl<'q, 's, 't> Trampoline<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let mut vdso = None;
        let mut vvar = None;
        let mut vsyscall = None;

        for map in MapsIterator::new(stopped_task) {
            match map.name {
                MemAreaName::VDSO => {
                    assert_eq!(map.read, true);
                    assert_eq!(map.write, false);
                    assert_eq!(map.execute, true);
                    assert_eq!(map.mayshare, false);
                    assert_eq!(map.dev_major, 0);
                    assert_eq!(map.dev_minor, 0);
                    assert_eq!(vdso, None);
                    vdso = Some(map);
                }
                MemAreaName::VVar => {
                    assert_eq!(map.read, true);
                    assert_eq!(map.write, false);
                    assert_eq!(map.execute, false);
                    assert_eq!(map.mayshare, false);
                    assert_eq!(map.dev_major, 0);
                    assert_eq!(map.dev_minor, 0);
                    assert_eq!(vvar, None);
                    vvar = Some(map);
                }
                MemAreaName::VSyscall => {
                    assert_eq!(map.read, false);
                    assert_eq!(map.write, false);
                    assert_eq!(map.execute, true);
                    assert_eq!(map.mayshare, false);
                    assert_eq!(map.dev_major, 0);
                    assert_eq!(map.dev_minor, 0);
                    assert_eq!(vsyscall, None);
                    vsyscall = Some(map);
                }
                _ => {}
            }
        }

        let vdso = vdso.unwrap();
        let vvar = vvar.unwrap();
        let vdso_syscall = find_syscall(stopped_task, &vdso).unwrap();

        Trampoline {
            stopped_task,
            vdso,
            vvar,
            vsyscall,
            vdso_syscall,
        }
    }

    pub async fn unmap_all_userspace_mem(&mut self) {
        loop {
            let mut to_unmap = None;
            for area in MapsIterator::new(self.stopped_task) {
                if area != self.vdso && area != self.vvar && Some(&area) != self.vsyscall.as_ref() {
                    to_unmap = Some(area);
                    break;
                }
            }
            match to_unmap {
                Some(area) => self.munmap(area.vptr(), area.len()).await.unwrap(),
                None => return,
            }
        }
    }

    pub async fn syscall(&mut self, nr: usize, args: &[isize]) -> isize {
        let task = &mut self.stopped_task.task;
        let pid = task.task_data.sys_pid;
        let task_regs = &mut self.stopped_task.regs;
        let mut local_regs = task_regs.clone();

        SyscallInfo::orig_nr_to_regs(nr as isize, &mut local_regs);
        SyscallInfo::args_to_regs(args, &mut local_regs);

        // Run the syscall until completion, trapping again on the way out
        ptrace::set_regs(pid, &local_regs);
        ptrace::trace_syscall(pid);
        assert_eq!(
            task.events.next().await,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_TRACESYSGOOD
            }
        );
        ptrace::get_regs(pid, &mut local_regs);

        // Save the results from the remote call
        let result = SyscallInfo::ret_from_regs(&mut local_regs);

        // Now we are trapped on the way out of a syscall but we need to get back to
        // trapping on the way in. This involves a brief trip back to userspace.
        // This can't be done without relying on userspace at all, as far as I
        // can tell, but we can reduce the dependency as much as possible by
        // using the VDSO as a trampoline.
        let fake_syscall_nr = sc::nr::OPEN;
        let fake_syscall_arg = 0xffff_ffff_dddd_dddd_u64;
        local_regs.ip = self.vdso_syscall.0 as u64;
        local_regs.sp = 0;
        SyscallInfo::nr_to_regs(fake_syscall_nr as isize, &mut local_regs);
        SyscallInfo::args_to_regs(&[fake_syscall_arg as isize; 6], &mut local_regs);

        ptrace::set_regs(pid, &local_regs);
        ptrace::single_step(pid);
        assert_eq!(
            task.events.next().await,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_SECCOMP
            }
        );
        ptrace::get_regs(pid, &mut local_regs);
        let info = SyscallInfo::from_regs(&local_regs);
        assert_eq!(info.nr, fake_syscall_nr as u64);
        assert_eq!(info.args, [fake_syscall_arg; 6]);

        ptrace::set_regs(pid, &task_regs);
        result
    }

    pub async fn mmap(
        &mut self,
        addr: VPtr,
        length: usize,
        prot: isize,
        flags: isize,
        fd: &RemoteFd,
        offset: usize,
    ) -> Result<VPtr, Errno> {
        let result = self
            .syscall(
                sc::nr::MMAP,
                &[
                    addr.0 as isize,
                    length as isize,
                    prot,
                    flags,
                    fd.0 as isize,
                    offset as isize,
                ],
            )
            .await;
        if result < 0 {
            Err(Errno(result as i32))
        } else {
            Ok(VPtr(result as usize))
        }
    }

    pub async fn munmap(&mut self, addr: VPtr, length: usize) -> Result<(), Errno> {
        let result = self
            .syscall(sc::nr::MUNMAP, &[addr.0 as isize, length as isize])
            .await;
        if result == 0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn close(&mut self, fd: &RemoteFd) -> Result<(), Errno> {
        let result = self.syscall(sc::nr::CLOSE, &[fd.0 as isize]).await;
        if result == 0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
        }
    }
}

pub fn mem_read_bytes<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    ptr: VPtr,
    bytes: &mut [u8],
) -> Result<(), ()> {
    let mem_fd = stopped_task.task.process_handle.mem.0;
    match unsafe {
        bytes.len()
            == syscall!(
                PREAD64,
                mem_fd,
                bytes.as_mut_ptr() as usize,
                bytes.len(),
                ptr.0
            )
    } {
        false => Err(()),
        true => Ok(()),
    }
}

/// safety: type must be repr(C) and have no invalid bit patterns
pub unsafe fn mem_read_value<T: Clone>(
    stopped_task: &mut StoppedTask,
    remote: VPtr,
) -> Result<T, ()> {
    let len = size_of::<T>();
    let mut storage = MaybeUninit::<T>::uninit();
    let byte_ref =
        core::slice::from_raw_parts_mut(&mut storage as *mut MaybeUninit<T> as *mut u8, len);
    mem_read_bytes(stopped_task, remote, byte_ref)?;
    let value_ref: &mut T = &mut *(byte_ref.as_mut_ptr() as *mut T);
    Ok(value_ref.clone())
}

pub fn mem_write_word<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    ptr: VPtr,
    word: usize,
) -> Result<(), ()> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    ptrace::poke(stopped_task.task.task_data.sys_pid, ptr.0, word)
}

pub fn mem_write_padded_bytes<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    mut ptr: VPtr,
    bytes: &[u8],
) -> Result<(), ()> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    for chunk in bytes.chunks(size_of::<usize>()) {
        let mut padded_chunk = 0usize.to_ne_bytes();
        padded_chunk[0..chunk.len()].copy_from_slice(chunk);
        mem_write_word(stopped_task, ptr, usize::from_ne_bytes(padded_chunk))?;
        ptr = ptr.add(padded_chunk.len());
    }
    Ok(())
}

/// safety: type must be repr(C)
pub unsafe fn mem_write_padded_value<'q, 's, T: Clone>(
    stopped_task: &mut StoppedTask<'q, 's>,
    remote: VPtr,
    local: &T,
) -> Result<(), ()> {
    // allocate aligned for T, explicitly zero all bytes, clone the value in, then
    // use as bytes again
    let len = size_of::<T>();
    let mut storage = MaybeUninit::<T>::uninit();
    let byte_ref =
        core::slice::from_raw_parts_mut(&mut storage as *mut MaybeUninit<T> as *mut u8, len);
    for byte in byte_ref.iter_mut() {
        *byte = 0;
    }
    let value_ref: &mut T = &mut *(byte_ref.as_mut_ptr() as *mut T);
    value_ref.clone_from(local);
    let byte_ref = core::slice::from_raw_parts(value_ref as *mut T as *mut u8, len);
    mem_write_padded_bytes(stopped_task, remote, byte_ref)
}

pub fn mem_find_bytes<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    ptr: VPtr,
    len: usize,
    pattern: &[u8],
) -> Result<VPtr, ()> {
    let mut buffer = [0u8; 4096];
    assert!(pattern.len() <= buffer.len());

    let mut chunk_offset = 0;
    loop {
        let chunk_size = buffer.len().min(len - chunk_offset);
        if chunk_size < pattern.len() {
            return Err(());
        }
        mem_read_bytes(stopped_task, ptr.add(chunk_offset), &mut buffer)?;
        if let Some(match_offset) = twoway::find_bytes(&buffer, pattern) {
            return Ok(ptr.add(chunk_offset).add(match_offset));
        }
        // overlap just enough to detect matches across chunk boundaries
        chunk_offset += chunk_size - pattern.len() + 1;
    }
}

#[derive(Debug)]
pub struct RemoteMemoryGuard;

impl Drop for RemoteMemoryGuard {
    fn drop(&mut self) {
        panic!("can't drop live remote memory")
    }
}

#[derive(Debug)]
pub struct Scratchpad<'q, 's, 't, 'r> {
    pub trampoline: &'r mut Trampoline<'q, 's, 't>,
    pub page_ptr: VPtr,
    mem: RemoteMemoryGuard,
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
            mem: RemoteMemoryGuard,
        })
    }

    pub async fn free(self) -> Result<(), Errno> {
        core::mem::forget(self.mem);
        self.trampoline.munmap(self.page_ptr, abi::PAGE_SIZE).await
    }

    pub async fn write_fd(&mut self, fd: &RemoteFd, bytes: &[u8]) -> Result<usize, Errno> {
        if bytes.len() > abi::PAGE_SIZE {
            return Err(Errno(-abi::EINVAL as i32));
        }
        mem_write_padded_bytes(self.trampoline.stopped_task, self.page_ptr, bytes)
            .map_err(|_| Errno(-abi::EFAULT as i32))?;
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
        unsafe { mem_write_padded_value(self.trampoline.stopped_task, self.page_ptr, duration) }
            .map_err(|_| Errno(-abi::EFAULT as i32))?;
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

        unsafe {
            mem_write_padded_value(self.trampoline.stopped_task, self.page_ptr, &remote_layout)
                .map_err(|_| Errno(-abi::EFAULT as i32))?;
        }

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

        let remote_cmsg: CMsg = unsafe {
            mem_read_value(
                self.trampoline.stopped_task,
                self.page_ptr.add(offset_of!(Layout, cmsg)),
            )
            .map_err(|_| Errno(-abi::EFAULT as i32))?
        };
        if remote_cmsg.hdr == BASE_LAYOUT.cmsg.hdr {
            Ok(RemoteFd(remote_cmsg.fd))
        } else {
            Err(Errno(-abi::EFAULT as i32))
        }
    }
}
