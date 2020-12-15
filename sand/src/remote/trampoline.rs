use crate::{
    abi,
    process::{
        maps::{KernelMemAreas, MapsIterator},
        task::StoppedTask,
        Event,
    },
    protocol::{abi::Syscall, Errno, LogLevel, LogMessage, VPtr},
    ptrace,
    remote::file::RemoteFd,
};

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
            for area in MapsIterator::new(self.stopped_task) {
                if self.kernel_mem.is_userspace_area(&area) {
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

    pub async fn mmap_anonymous_noreplace(
        &mut self,
        addr: VPtr,
        length: usize,
        prot: isize,
    ) -> Result<(), Errno> {
        let flags = abi::MAP_PRIVATE | abi::MAP_ANONYMOUS | abi::MAP_FIXED_NOREPLACE;
        let result = self
            .mmap(addr, length, prot, flags, &RemoteFd(0), 0)
            .await?;
        if result == addr {
            Ok(())
        } else {
            // kernel might not understand MAP_FIXED_NOREPLACE, it moved the mapping. undo.
            self.munmap(result, length).await?;
            Err(Errno(-abi::EEXIST))
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

    pub async fn mremap(
        &mut self,
        addr: VPtr,
        old_length: usize,
        new_length: usize,
    ) -> Result<(), Errno> {
        let result = self
            .syscall(
                sc::nr::MREMAP,
                &[addr.0 as isize, old_length as isize, new_length as isize, 0],
            )
            .await;
        if result as usize == addr.0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
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
