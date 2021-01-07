use crate::{
    protocol::{Errno, SysFd, VPid, VPtr},
    remote::{
        file::{RemoteFd, TempRemoteFd},
        scratchpad::Scratchpad,
        trampoline::Trampoline,
    },
};
use core::convert::From;

pub struct SyscallResult(pub isize);

impl From<Errno> for SyscallResult {
    fn from(err: Errno) -> Self {
        let result = SyscallResult(err.0 as isize);
        assert!(result.0 < 0);
        result
    }
}

impl<T: Into<SyscallResult>, U: Into<SyscallResult>> From<Result<T, U>> for SyscallResult {
    fn from(result: Result<T, U>) -> SyscallResult {
        match result {
            Ok(r) => r.into(),
            Err(r) => r.into(),
        }
    }
}

impl From<()> for SyscallResult {
    fn from(_: ()) -> Self {
        SyscallResult(0)
    }
}

impl From<VPtr> for SyscallResult {
    fn from(ptr: VPtr) -> Self {
        let result = SyscallResult(ptr.0 as isize);
        assert!(result.0 >= 0);
        result
    }
}

impl From<VPid> for SyscallResult {
    fn from(pid: VPid) -> Self {
        let result = SyscallResult(pid.0 as isize);
        assert!(result.0 >= 0);
        result
    }
}

impl From<RemoteFd> for SyscallResult {
    fn from(fd: RemoteFd) -> Self {
        let result = SyscallResult(fd.0 as isize);
        assert!(result.0 >= 0);
        result
    }
}

impl From<usize> for SyscallResult {
    fn from(s: usize) -> Self {
        let result = SyscallResult(s as isize);
        assert!(result.0 >= 0);
        result
    }
}

pub async fn file(trampoline: &mut Trampoline<'_, '_, '_>, fd: &SysFd) -> Result<RemoteFd, Errno> {
    let mut pad = Scratchpad::new(trampoline).await?;
    let main_result = RemoteFd::from_local(&mut pad, fd).await;
    let cleanup_result = pad.free().await;
    let result = main_result?;
    cleanup_result?;
    Ok(result)
}

pub async fn sysfd_bytes(
    trampoline: &mut Trampoline<'_, '_, '_>,
    from_fd: &SysFd,
    to_ptr: VPtr,
    len: usize,
) -> Result<(), Errno> {
    let remote_fd = file(trampoline, from_fd).await?;
    let main_result = remote_fd.pread_vptr_exact(trampoline, to_ptr, len, 0).await;
    let cleanup_result = remote_fd.close(trampoline).await;
    main_result?;
    cleanup_result?;
    Ok(())
}

pub async fn local_bytes(
    trampoline: &mut Trampoline<'_, '_, '_>,
    bytes: &[u8],
    to_ptr: VPtr,
) -> Result<(), Errno> {
    let mut pad = Scratchpad::new(trampoline).await?;
    let main_result = local_bytes_with_scratchpad(&mut pad, bytes, to_ptr).await;
    let cleanup_result = pad.free().await;
    main_result?;
    cleanup_result?;
    Ok(())
}

pub async fn local_bytes_with_scratchpad(
    scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
    bytes: &[u8],
    to_ptr: VPtr,
) -> Result<(), Errno> {
    let remote_fd = TempRemoteFd::new(scratchpad).await?;
    let main_result = remote_fd
        .mem_write_bytes_exact(scratchpad, to_ptr, bytes)
        .await;
    let cleanup_result = remote_fd.free(&mut scratchpad.trampoline).await;
    main_result?;
    cleanup_result?;
    Ok(())
}
