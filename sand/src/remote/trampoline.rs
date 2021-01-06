use crate::{
    abi,
    mem::{
        kernel::{KernelMemAreas, KernelMemIterator},
        maps::{MappedPages, MemFlags},
        page::VPage,
    },
    process::{task::StoppedTask, Event},
    protocol::{abi::Syscall, Errno, LogLevel, LogMessage, VPtr},
    ptrace,
    remote::file::RemoteFd,
};
use core::ops::Range;

#[derive(Debug)]
pub struct Trampoline<'q, 's, 't> {
    pub stopped_task: &'t mut StoppedTask<'q, 's>,
    pub kernel_mem: KernelMemAreas,
}

impl<'q, 's, 't> Trampoline<'q, 's, 't> {
    pub fn new(stopped_task: &'t mut StoppedTask<'q, 's>) -> Self {
        let kernel_mem = KernelMemAreas::locate(stopped_task);
        Trampoline {
            stopped_task,
            kernel_mem,
        }
    }

    pub async fn unmap_all_userspace_mem(&mut self) {
        loop {
            let mut to_unmap = None;
            for area in KernelMemIterator::new(self.stopped_task) {
                if self.kernel_mem.is_userspace_area(&area) {
                    to_unmap = Some(area);
                    break;
                }
            }
            match to_unmap {
                Some(area) => self
                    .munmap(&area.pages.mem_pages())
                    .await
                    .expect("unmap userspace"),
                None => return,
            }
        }
    }

    pub async fn syscall(&mut self, nr: usize, args: &[isize]) -> isize {
        let pid = self.stopped_task.task.task_data.sys_pid;
        let mut local_regs = self.stopped_task.regs.clone();

        Syscall::orig_nr_to_regs(nr as isize, &mut local_regs);
        Syscall::args_to_regs(args, &mut local_regs);

        // Run the syscall until completion, trapping again on the way out
        ptrace::set_regs(pid, &local_regs);
        ptrace::trace_syscall(pid);
        self.stopped_task
            .expect_event_or_panic(Event::Signal {
                sig: abi::SIGCHLD as u32,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_TRACESYSGOOD,
            })
            .await;
        ptrace::get_regs(pid, &mut local_regs);

        // Save the results from the remote call
        let result = Syscall::ret_from_regs(&local_regs);

        let log_level = LogLevel::Debug;
        if self.stopped_task.task.log_enabled(log_level) {
            self.stopped_task.task.log(
                log_level,
                LogMessage::Remote(Syscall::from_regs(&local_regs)),
            )
        }

        // Now we are trapped on the way out of a syscall but we need to get back to
        // trapping on the way in. This involves a brief trip back to userspace.
        // This can't be done without relying on userspace at all, as far as I
        // can tell, but we can reduce the dependency as much as possible by
        // using the VDSO as a trampoline.
        let fake_syscall_nr = sc::nr::OPEN as isize;
        let fake_syscall_arg = 0xffff_ffff_dddd_dddd_u64 as isize;
        local_regs.ip = self.kernel_mem.vdso_syscall.0;
        local_regs.sp = 0;
        Syscall::nr_to_regs(fake_syscall_nr, &mut local_regs);
        Syscall::args_to_regs(&[fake_syscall_arg; 6], &mut local_regs);

        ptrace::set_regs(pid, &local_regs);
        ptrace::single_step(pid);
        self.stopped_task
            .expect_event_or_panic(Event::Signal {
                sig: abi::SIGCHLD as u32,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_SECCOMP,
            })
            .await;
        ptrace::get_regs(pid, &mut local_regs);
        let info = Syscall::from_regs(&local_regs);
        assert_eq!(info.nr, fake_syscall_nr);
        assert_eq!(info.args, [fake_syscall_arg; 6]);

        ptrace::set_regs(pid, &self.stopped_task.regs);
        result
    }

    pub async fn mmap(
        &mut self,
        mapped_pages: &MappedPages,
        fd: &RemoteFd,
        mem_flags: &MemFlags,
        map_flags: isize,
    ) -> Result<Range<VPage>, Errno> {
        let len = mapped_pages.mem_range().end.0 - mapped_pages.mem_range().start.0;
        let result = self
            .syscall(
                sc::nr::MMAP,
                &[
                    mapped_pages.mem_pages().start.ptr().0 as isize,
                    len as isize,
                    mem_flags.protect.prot_flags(),
                    mem_flags.map_flags() | map_flags,
                    fd.0 as isize,
                    mapped_pages.file_start() as isize,
                ],
            )
            .await;
        if result < 0 {
            Err(Errno(result as i32))
        } else {
            let result = VPtr(result as usize);
            VPage::parse_range(&(result..(result + len))).map_err(|()| Errno(-abi::EINVAL))
        }
    }

    pub async fn mmap_fixed(
        &mut self,
        mapped_pages: &MappedPages,
        fd: &RemoteFd,
        mem_flags: &MemFlags,
        map_flags: isize,
    ) -> Result<(), Errno> {
        let expected = mapped_pages.mem_pages();
        let result = self.mmap(mapped_pages, fd, mem_flags, map_flags).await?;
        if expected == result {
            Ok(())
        } else {
            // Unexpected location, unmap the unwanted mapping before failing.
            self.munmap(&result).await?;
            Err(Errno(-abi::EINVAL))
        }
    }

    pub async fn munmap(&mut self, pages: &Range<VPage>) -> Result<(), Errno> {
        let result = self
            .syscall(
                sc::nr::MUNMAP,
                &[
                    pages.start.ptr().0 as isize,
                    (pages.end.ptr().0 - pages.start.ptr().0) as isize,
                ],
            )
            .await;
        if result == 0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn mremap(
        &mut self,
        pages: &Range<VPage>,
        new_length: usize,
    ) -> Result<Range<VPage>, Errno> {
        let result = self
            .syscall(
                sc::nr::MREMAP,
                &[
                    pages.start.ptr().0 as isize,
                    (pages.end.ptr().0 - pages.start.ptr().0) as isize,
                    new_length as isize,
                    0,
                ],
            )
            .await;
        if result < 0 {
            Err(Errno(result as i32))
        } else {
            let new_addr = VPtr(result as usize);
            VPage::parse_range(&(new_addr..(new_addr + new_length)))
                .map_err(|()| Errno(-abi::EINVAL))
        }
    }

    pub async fn getrandom(
        &mut self,
        addr: VPtr,
        length: usize,
        flags: isize,
    ) -> Result<usize, Errno> {
        let result = self
            .syscall(
                sc::nr::GETRANDOM,
                &[addr.0 as isize, length as isize, flags],
            )
            .await;
        if result >= 0 {
            Ok(result as usize)
        } else {
            Err(Errno(result as i32))
        }
    }

    pub async fn getrandom_exact(
        &mut self,
        addr: VPtr,
        length: usize,
        flags: isize,
    ) -> Result<(), Errno> {
        if self.getrandom(addr, length, flags).await? == length {
            Ok(())
        } else {
            Err(Errno(-abi::EIO))
        }
    }
}
