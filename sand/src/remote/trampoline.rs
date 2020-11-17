use crate::{
    abi,
    abi::SyscallInfo,
    process::{
        maps::{MapsIterator, MemArea, MemAreaName},
        task::StoppedTask,
        Event,
    },
    protocol::{Errno, VPtr},
    ptrace,
    remote::{mem::find_bytes, RemoteFd},
};

#[derive(Debug)]
pub struct Trampoline<'q, 's, 't> {
    pub stopped_task: &'t mut StoppedTask<'q, 's>,
    pub vdso: MemArea,
    pub vvar: MemArea,
    pub vsyscall: Option<MemArea>,
    pub vdso_syscall: VPtr,
    pub task_end: VPtr,
}

fn find_syscall<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    vdso: &MemArea,
) -> Result<VPtr, ()> {
    const X86_64_SYSCALL: [u8; 2] = [0x0f, 0x05];
    find_bytes(
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
        let mut task_end = !0usize;

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
                    task_end = task_end.min(map.start);
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
                    task_end = task_end.min(map.start);
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
                    task_end = task_end.min(map.start);
                    vsyscall = Some(map);
                }
                _ => {}
            }
        }

        let vdso = vdso.unwrap();
        let vvar = vvar.unwrap();
        let vdso_syscall = find_syscall(stopped_task, &vdso).unwrap();
        let task_end = VPtr(task_end);

        Trampoline {
            stopped_task,
            vdso,
            vvar,
            vsyscall,
            vdso_syscall,
            task_end,
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

    pub async fn pread(
        &mut self,
        fd: &RemoteFd,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<usize, Errno> {
        let result = self
            .syscall(
                sc::nr::PREAD64,
                &[
                    fd.0 as isize,
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

    pub async fn pread_exact(
        &mut self,
        fd: &RemoteFd,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<(), Errno> {
        match self.pread(fd, addr, length, offset).await {
            Ok(actual) if actual == length => Ok(()),
            Ok(_) => Err(Errno(-abi::EIO)),
            Err(e) => Err(e),
        }
    }

    pub async fn pwrite(
        &mut self,
        fd: &RemoteFd,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<usize, Errno> {
        let result = self
            .syscall(
                sc::nr::PWRITE64,
                &[
                    fd.0 as isize,
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

    pub async fn pwrite_exact(
        &mut self,
        fd: &RemoteFd,
        addr: VPtr,
        length: usize,
        offset: usize,
    ) -> Result<(), Errno> {
        match self.pwrite(fd, addr, length, offset).await {
            Ok(actual) if actual == length => Ok(()),
            Ok(_) => Err(Errno(-abi::EIO)),
            Err(e) => Err(e),
        }
    }
}
