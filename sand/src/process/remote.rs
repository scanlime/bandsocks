use crate::{
    abi,
    abi::SyscallInfo,
    process::{
        maps::{MapsIterator, MemArea, MemAreaName},
        task::StoppedTask,
        Event,
    },
    protocol::VPtr,
    ptrace,
};
use core::mem::size_of;
use sc::syscall;

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
    mem_find(
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
        let regs = &mut self.stopped_task.regs;
        let pid = task.task_data.sys_pid;

        SyscallInfo::orig_nr_to_regs(nr as isize, regs);
        SyscallInfo::args_to_regs(args, regs);

        // Run the syscall until completion, trapping again on the way out
        ptrace::set_regs(pid, regs);
        ptrace::trace_syscall(pid);
        assert_eq!(
            task.events.next().await,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_TRACESYSGOOD
            }
        );
        ptrace::get_regs(pid, regs);

        // Save the results from the remote call
        let result = SyscallInfo::ret_from_regs(regs);

        // Now we are trapped on the way out of a syscall but we need to get back to
        // trapping on the way in. This involves a brief trip back to userspace.
        // This can't be done without relying on userspace at all, as far as I
        // can tell, but we can reduce the dependency as much as possible by
        // using the VDSO as a trampoline.
        let fake_syscall_nr = sc::nr::OPEN;
        let fake_syscall_arg = 0xffff_ffff_dddd_dddd_u64;
        regs.ip = self.vdso_syscall.0 as u64;
        regs.sp = 0;
        SyscallInfo::nr_to_regs(fake_syscall_nr as isize, regs);
        SyscallInfo::args_to_regs(&[fake_syscall_arg as isize; 6], regs);

        ptrace::set_regs(pid, regs);
        ptrace::single_step(pid);
        assert_eq!(
            task.events.next().await,
            Event::Signal {
                sig: abi::SIGCHLD,
                code: abi::CLD_TRAPPED,
                status: abi::PTRACE_SIG_SECCOMP
            }
        );
        ptrace::get_regs(pid, regs);
        let info = SyscallInfo::from_regs(regs);
        assert_eq!(info.nr, fake_syscall_nr as u64);
        assert_eq!(info.args, [fake_syscall_arg; 6]);

        result
    }

    pub async fn mmap(
        &mut self,
        addr: VPtr,
        length: usize,
        prot: isize,
        flags: isize,
        fd: isize,
        offset: isize,
    ) -> Result<VPtr, ()> {
        let result = self
            .syscall(
                sc::nr::MMAP,
                &[addr.0 as isize, length as isize, prot, flags, fd, offset],
            )
            .await;
        if result == abi::MAP_FAILED {
            Err(())
        } else {
            Ok(VPtr(result as usize))
        }
    }

    pub async fn munmap(&mut self, addr: VPtr, length: usize) -> Result<(), ()> {
        let result = self
            .syscall(sc::nr::MUNMAP, &[addr.0 as isize, length as isize])
            .await;
        if result == 0 {
            Ok(())
        } else {
            Err(())
        }
    }
}

pub fn mem_read<'q, 's>(
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

pub fn mem_write_word<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    ptr: VPtr,
    word: usize,
) -> Result<(), ()> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    ptrace::poke(stopped_task.task.task_data.sys_pid, ptr.0, word)
}

pub fn mem_write_words<'q, 's>(
    stopped_task: &mut StoppedTask<'q, 's>,
    mut ptr: VPtr,
    words: &[usize],
) -> Result<(), ()> {
    assert!(0 == (ptr.0 % size_of::<usize>()));
    for word in words {
        mem_write_word(stopped_task, ptr, *word)?;
        ptr = ptr.add(size_of::<usize>());
    }
    Ok(())
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

pub fn mem_find<'q, 's>(
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
        mem_read(stopped_task, ptr.add(chunk_offset), &mut buffer)?;
        if let Some(match_offset) = twoway::find_bytes(&buffer, pattern) {
            return Ok(ptr.add(chunk_offset).add(match_offset));
        }
        // overlap just enough to detect matches across chunk boundaries
        chunk_offset += chunk_size - pattern.len() + 1;
    }
}
