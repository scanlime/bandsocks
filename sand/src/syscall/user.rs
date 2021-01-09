use crate::{
    abi,
    mem::{
        maps::{MappedPages, MemFlags},
        page::VPage,
    },
    process::task::StoppedTask,
    protocol::{Errno, VPtr},
    remote::{
        file::{RemoteFd, TempRemoteFd},
        scratchpad::Scratchpad,
        trampoline::Trampoline,
    },
    syscall::result::SyscallResult,
};

pub async fn uname<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    dest: VPtr,
) -> Result<(), Errno> {
    let mut tr = Trampoline::new(stopped_task);
    let mut pad = Scratchpad::new(&mut tr).await?;
    let main_result = match TempRemoteFd::new(&mut pad).await {
        Err(err) => Err(err),
        Ok(temp) => {
            let main_result = Ok(());
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, sysname),
                    b"Linux\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, nodename),
                    b"host\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, release),
                    b"4.0.0-bandsocks\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, version),
                    b"#1 SMP\0",
                )
                .await,
            );
            let main_result = main_result.and(
                temp.mem_write_bytes_exact(
                    &mut pad,
                    dest + offset_of!(abi::UtsName, machine),
                    abi::PLATFORM_NAME_BYTES,
                )
                .await,
            );

            let cleanup_result = temp.free(&mut pad.trampoline).await;
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

/// brk() is emulated using mmap because we can't change the host kernel's per
/// process brk pointer from our loader without extra privileges.
pub async fn brk<'q, 's, 't>(
    stopped_task: &'t mut StoppedTask<'q, 's>,
    new_brk: VPtr,
) -> Result<VPtr, Errno> {
    if new_brk.0 != 0 {
        let old_brk = stopped_task.task.task_data.mm.brk;
        let brk_start = stopped_task.task.task_data.mm.brk_start;
        let old_brk_page = VPage::round_up(brk_start.ptr().max(old_brk));
        let new_brk_page = VPage::round_up(brk_start.ptr().max(new_brk));

        if new_brk_page != old_brk_page {
            let mut tr = Trampoline::new(stopped_task);

            if new_brk_page == brk_start {
                tr.munmap(&(brk_start..old_brk_page)).await?;
            } else if old_brk_page == brk_start {
                tr.mmap(
                    &MappedPages::anonymous(brk_start..new_brk_page),
                    &RemoteFd::invalid(),
                    &MemFlags::rw(),
                    abi::MAP_ANONYMOUS,
                )
                .await?;
            } else {
                tr.mremap(
                    &(brk_start..old_brk_page),
                    new_brk_page.ptr().0 - brk_start.ptr().0,
                )
                .await?;
            }
        }
        stopped_task.task.task_data.mm.brk = brk_start.ptr().max(new_brk);
    }
    Ok(stopped_task.task.task_data.mm.brk)
}

pub async fn fork(stopped_task: &mut StoppedTask<'_, '_>) -> SyscallResult {
    let mut tr = Trampoline::new(stopped_task);
    // to do:
    //   pid translate, allocate task
    //   expect ptrace fork event
    //     probably requires lower-level trampoline remote syscall interface
    //
    // current test case:
    // $ cargo run --release busybox:musl -- sh -c "true&"
    SyscallResult(tr.syscall(sc::nr::FORK, &[]).await)
}
