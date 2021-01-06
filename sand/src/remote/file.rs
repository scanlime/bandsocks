use crate::{
    abi,
    abi::{CMsgHdr, IOVec, MsgHdr},
    mem::{
        maps::{MappedPages, MemFlags, Segment},
        page::VPage,
        rw::{read_value, write_padded_bytes, write_padded_value},
    },
    protocol::{Errno, SysFd, VPtr},
    remote::{scratchpad::Scratchpad, trampoline::Trampoline},
};
use core::{mem::size_of, pin::Pin, ptr};
use sc::{nr, syscall};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub struct RemoteFd(pub u32);

impl RemoteFd {
    pub fn invalid() -> RemoteFd {
        RemoteFd(!0)
    }

    pub async fn memfd_create(
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        name: &[u8],
        flags: isize,
    ) -> Result<RemoteFd, Errno> {
        let name_start = scratchpad.mem_range.start.ptr();
        let name_end = name_start + name.len();
        if name_end >= scratchpad.mem_range.end.ptr() {
            return Err(Errno(-abi::EINVAL));
        }
        write_padded_bytes(scratchpad.trampoline.stopped_task, name_start, name)?;
        let result = scratchpad
            .trampoline
            .syscall(nr::MEMFD_CREATE, &[name_start.0 as isize, flags as isize])
            .await;
        if result >= 0 {
            Ok(RemoteFd(result as u32))
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn from_local(
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        local: &SysFd,
    ) -> Result<RemoteFd, Errno> {
        let socket_pair = &scratchpad
            .trampoline
            .stopped_task
            .task
            .task_data
            .socket_pair;
        let local_socket_fd = socket_pair.tracer.fd.0;
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

        remote_layout.hdr.msg_iov = (scratchpad.ptr() + offset_of!(Layout, iov)).0 as *mut IOVec;
        remote_layout.hdr.msg_control =
            (scratchpad.ptr() + offset_of!(Layout, cmsg)).0 as *mut usize;
        remote_layout.iov.base = (scratchpad.ptr() + offset_of!(Layout, msg)).0 as *mut u8;

        unsafe {
            write_padded_value(
                scratchpad.trampoline.stopped_task,
                scratchpad.ptr(),
                &remote_layout,
            )
        }?;

        let local_flags = abi::MSG_DONTWAIT;
        let remote_flags = 0isize;

        let local_result =
            unsafe { syscall!(SENDMSG, local_socket_fd, local_ptr, local_flags) as isize };
        if local_result != 1 {
            return Err(Errno(local_result as i32));
        }

        let remote_result = scratchpad
            .trampoline
            .syscall(
                nr::RECVMSG,
                &[
                    remote_socket_fd as isize,
                    scratchpad.ptr().0 as isize,
                    remote_flags,
                ],
            )
            .await;
        if remote_result != 1 {
            return Err(Errno(remote_result as i32));
        }

        let remote_cmsg: CMsg = unsafe {
            read_value(
                scratchpad.trampoline.stopped_task,
                scratchpad.ptr() + offset_of!(Layout, cmsg),
            )
        }?;
        if remote_cmsg.hdr == BASE_LAYOUT.cmsg.hdr {
            Ok(RemoteFd(remote_cmsg.fd))
        } else {
            Err(Errno(-abi::EIO))
        }
    }

    pub async fn close(&self, tr: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        let result = tr.syscall(sc::nr::CLOSE, &[self.0 as isize]).await;
        if result == 0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn pread_vptr(
        &self,
        tr: &mut Trampoline<'_, '_, '_>,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<usize, Errno> {
        let result = tr
            .syscall(
                sc::nr::PREAD64,
                &[
                    self.0 as isize,
                    addr.0 as isize,
                    length as isize,
                    offset as isize,
                ],
            )
            .await;
        if result >= 0 {
            Ok(result as usize)
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn pread_vptr_exact(
        &self,
        tr: &mut Trampoline<'_, '_, '_>,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<(), Errno> {
        match self.pread_vptr(tr, addr, length, offset).await {
            Ok(actual) if actual == length => Ok(()),
            Ok(_) => Err(Errno(-abi::EIO)),
            Err(e) => Err(e),
        }
    }

    pub async fn pwrite_bytes_exact(
        &self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        bytes: &[u8],
        offset: usize,
    ) -> Result<(), Errno> {
        if bytes.len() > scratchpad.len() - size_of::<usize>() {
            return Err(Errno(-abi::EINVAL));
        }
        write_padded_bytes(scratchpad.trampoline.stopped_task, scratchpad.ptr(), bytes)?;
        self.pwrite_vptr_exact(scratchpad.trampoline, scratchpad.ptr(), bytes.len(), offset)
            .await
    }

    pub async fn pwrite_vptr(
        &self,
        tr: &mut Trampoline<'_, '_, '_>,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<usize, Errno> {
        let result = tr
            .syscall(
                sc::nr::PWRITE64,
                &[
                    self.0 as isize,
                    addr.0 as isize,
                    length as isize,
                    offset as isize,
                ],
            )
            .await;
        if result >= 0 {
            Ok(result as usize)
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn pwrite_vptr_exact(
        &self,
        tr: &mut Trampoline<'_, '_, '_>,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<(), Errno> {
        match self.pwrite_vptr(tr, addr, length, offset).await {
            Ok(actual) if actual == length => Ok(()),
            Ok(_) => Err(Errno(-abi::EIO)),
            Err(e) => Err(e),
        }
    }
}

#[derive(Debug)]
pub struct EmptyTempRemoteFd(TempRemoteFd);

impl EmptyTempRemoteFd {
    pub async fn new(scratchpad: &mut Scratchpad<'_, '_, '_, '_>) -> Result<Self, Errno> {
        Ok(EmptyTempRemoteFd(TempRemoteFd(
            RemoteFd::memfd_create(scratchpad, b"bandsocks-temp\0", 0).await?,
        )))
    }
}

#[derive(Debug)]
pub struct TempRemoteFd(pub RemoteFd);

impl Drop for TempRemoteFd {
    fn drop(&mut self) {
        panic!("leaking remote temp fd {:?}", self)
    }
}

impl TempRemoteFd {
    pub async fn new(scratchpad: &mut Scratchpad<'_, '_, '_, '_>) -> Result<TempRemoteFd, Errno> {
        Ok(EmptyTempRemoteFd::new(scratchpad).await?.0)
    }

    pub async fn free(self, trampoline: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        self.0.close(trampoline).await?;
        core::mem::forget(self);
        Ok(())
    }

    /// use this temp memfd to perform a remote memmove() operation
    pub async fn memmove(
        &self,
        trampoline: &mut Trampoline<'_, '_, '_>,
        dest: VPtr,
        src: VPtr,
        len: usize,
    ) -> Result<(), Errno> {
        let result = self.0.pwrite_vptr_exact(trampoline, src, len, 0).await;
        let result = result.and(self.0.pread_vptr_exact(trampoline, dest, len, 0).await);
        result
    }

    /// unlike write_padded_bytes, this can be an unaligned buffer of
    /// unaligned length
    pub async fn mem_write_bytes_exact(
        &self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        addr: VPtr,
        bytes: &[u8],
    ) -> Result<(), Errno> {
        if bytes.len() > scratchpad.len() - size_of::<usize>() {
            return Err(Errno(-abi::EINVAL));
        }
        write_padded_bytes(scratchpad.trampoline.stopped_task, scratchpad.ptr(), bytes)?;
        self.memmove(scratchpad.trampoline, addr, scratchpad.ptr(), bytes.len())
            .await
    }
}

#[derive(Debug)]
pub struct ZeroRemoteFd {
    inner: TempRemoteFd,
    capacity: usize,
}

impl ZeroRemoteFd {
    pub async fn new(scratchpad: &mut Scratchpad<'_, '_, '_, '_>) -> Result<ZeroRemoteFd, Errno> {
        Ok(ZeroRemoteFd {
            inner: EmptyTempRemoteFd::new(scratchpad).await?.0,
            capacity: 0,
        })
    }

    pub async fn free(self, trampoline: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        self.inner.free(trampoline).await
    }

    pub async fn ensure_capacity(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        min_capacity: usize,
    ) -> Result<(), Errno> {
        if min_capacity > self.capacity {
            self.inner
                .0
                .pwrite_bytes_exact(scratchpad, &[0], min_capacity - 1)
                .await?;
            self.capacity = min_capacity;
        }
        Ok(())
    }

    pub async fn memzero(
        &mut self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        addr: VPtr,
        len: usize,
    ) -> Result<(), Errno> {
        self.ensure_capacity(scratchpad, len).await?;
        self.inner
            .0
            .pread_vptr_exact(scratchpad.trampoline, addr, len, 0)
            .await
    }
}

pub async fn fd_copy_via_buffer(
    scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
    src_fd: &RemoteFd,
    mut src_offset: usize,
    dest_fd: &RemoteFd,
    mut dest_offset: usize,
    mut byte_count: usize,
) -> Result<usize, Errno> {
    let mut transferred = 0;
    while byte_count > 0 {
        let part_size = byte_count.min(scratchpad.len());
        let read_len = src_fd
            .pread_vptr(
                scratchpad.trampoline,
                scratchpad.ptr(),
                part_size,
                src_offset,
            )
            .await?;
        if read_len > 0 {
            let write_len = dest_fd
                .pwrite_vptr(
                    scratchpad.trampoline,
                    scratchpad.ptr(),
                    read_len,
                    dest_offset,
                )
                .await?;
            transferred += write_len;
            if write_len == part_size {
                byte_count -= part_size;
                src_offset += part_size;
                dest_offset += part_size;
                continue;
            }
        }
        break;
    }
    Ok(transferred)
}

pub async fn fd_copy_exact(
    scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
    src_fd: &RemoteFd,
    src_offset: usize,
    dest_fd: &RemoteFd,
    dest_offset: usize,
    byte_count: usize,
) -> Result<(), Errno> {
    if fd_copy_via_buffer(
        scratchpad,
        src_fd,
        src_offset,
        dest_fd,
        dest_offset,
        byte_count,
    )
    .await?
        == byte_count
    {
        Ok(())
    } else {
        Err(Errno(-abi::EIO))
    }
}

pub async fn memzero(
    trampoline: &mut Trampoline<'_, '_, '_>,
    addr: VPtr,
    len: usize,
) -> Result<(), Errno> {
    let mut pad = Scratchpad::new(trampoline).await?;
    let main_result = match ZeroRemoteFd::new(&mut pad).await {
        Err(err) => Err(err),
        Ok(mut zerofd) => {
            let main_result = zerofd.memzero(&mut pad, addr, len).await;
            let cleanup_result = zerofd.free(&mut pad.trampoline).await;
            match (main_result, cleanup_result) {
                (Ok(r), Ok(())) => Ok(r),
                (Err(e), _) => Err(e),
                (Ok(_), Err(e)) => Err(e),
            }
        }
    };
    let cleanup_result = pad.free().await;
    main_result?;
    cleanup_result?;
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub enum MapLocation {
    Arbitrary,
    Offset(VPage),
}

#[derive(Debug)]
pub struct LoadedSegment(Segment);

impl LoadedSegment {
    pub async fn new(
        trampoline: &mut Trampoline<'_, '_, '_>,
        file: &RemoteFd,
        segment: &Segment,
        location: &MapLocation,
    ) -> Result<LoadedSegment, Errno> {
        let mem_flags = MemFlags {
            protect: segment.protect.clone(),
            mayshare: false,
        };
        // Map anonymous memory to allocate the full region, and relocate as requested
        let segment = match location {
            MapLocation::Arbitrary => segment.clone().set_page_start(
                trampoline
                    .mmap(
                        &MappedPages::anonymous(
                            segment.clone().set_page_start(VPage::null()).mem_pages(),
                        ),
                        &RemoteFd::invalid(),
                        &mem_flags,
                        abi::MAP_ANONYMOUS,
                    )
                    .await?
                    .start,
            ),
            MapLocation::Offset(page) => {
                let segment = segment.clone().offset(page.ptr().0);
                trampoline
                    .mmap_fixed(
                        &MappedPages::anonymous(segment.mem_pages()),
                        &RemoteFd::invalid(),
                        &mem_flags,
                        abi::MAP_ANONYMOUS | abi::MAP_FIXED_NOREPLACE,
                    )
                    .await?;
                segment
            }
        };
        if !segment.mem_pages().is_empty() {
            let mapped_range = &segment.mapped_range;
            let mapped_pages = segment.mapped_pages();

            // Map the page-aligned region around the file contents
            trampoline
                .mmap_fixed(&mapped_pages, file, &mem_flags, abi::MAP_FIXED)
                .await?;

            // Might need to additionally zero the tail end of the file mapping, at the
            // boundary between rwdata and bss segments
            if segment.protect.write && mapped_range.mem.end < mapped_pages.mem_pages().end.ptr() {
                memzero(
                    trampoline,
                    mapped_range.mem.end,
                    mapped_pages.mem_pages().end.ptr().0 - mapped_range.mem.end.0,
                )
                .await?;
            }
        }

        Ok(LoadedSegment(segment))
    }

    pub async fn free(self, trampoline: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        trampoline.munmap(&self.0.mem_pages()).await
    }

    pub fn segment(&self) -> &Segment {
        &self.0
    }
}
