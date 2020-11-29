use crate::{
    abi,
    abi::SyscallInfo,
    process::{
        maps::{MapsIterator, MemArea, MemAreaName},
        task::{StoppedTask, Task},
        Event,
    },
    protocol::{Errno, LogLevel, LogMessage, LogSyscall, VPtr},
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
        let pid = self.stopped_task.task.task_data.sys_pid;
        let mut local_regs = self.stopped_task.regs.clone();

        SyscallInfo::orig_nr_to_regs(nr as isize, &mut local_regs);
        SyscallInfo::args_to_regs(args, &mut local_regs);
        let call = SyscallInfo::from_regs(&local_regs);

        // Run the syscall until completion, trapping again on the way out
        ptrace::set_regs(pid, &local_regs);
        ptrace::trace_syscall(pid);
        self.stopped_task
            .expect_event_or_panic(Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_TRACESYSGOOD,
            })
            .await;
        ptrace::get_regs(pid, &mut local_regs);

        // Save the results from the remote call
        let result = SyscallInfo::ret_from_regs(&local_regs);

        let log_level = LogLevel::Debug;
        if self.stopped_task.task.log_enabled(log_level) {
            self.stopped_task.task.log(
                log_level,
                LogMessage::Remote(LogSyscall(call.nr, call.args, result)),
            )
        }

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
        self.stopped_task
            .expect_event_or_panic(Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_SECCOMP,
            })
            .await;
        ptrace::get_regs(pid, &mut local_regs);
        let info = SyscallInfo::from_regs(&local_regs);
        assert_eq!(info.nr, fake_syscall_nr as u64);
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

    pub async fn mmap_anonymous(
        &mut self,
        addr: VPtr,
        length: usize,
        prot: isize,
    ) -> Result<VPtr, Errno> {
        let flags = abi::MAP_PRIVATE | abi::MAP_ANONYMOUS | abi::MAP_FIXED;
        self.mmap(addr, length, prot, flags, &RemoteFd(0), 0).await
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
