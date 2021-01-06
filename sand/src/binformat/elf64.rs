use crate::{
    abi,
    binformat::{Exec, ExecFile, FileHeader},
    mem::{
        maps::{MappedRange, MemProtect, Segment},
        page::{page_offset, VPage},
        string::VStringRange,
    },
    nolibc,
    process::{
        stack::{InitialStack, StackBuilder},
        task::StoppedTask,
    },
    protocol::{abi::UserRegs, Errno, VPtr},
    remote::{
        file::{LoadedSegment, MapLocation, RemoteFd, TempRemoteFd},
        scratchpad::Scratchpad,
        trampoline::Trampoline,
    },
};
use core::{
    mem::{size_of, size_of_val},
    ops::Range,
};
use goblin::elf64::{header, header::Header, program_header, program_header::ProgramHeader};

pub fn detect(fh: &FileHeader) -> bool {
    let ehdr = elf_header(fh);
    let magic = &ehdr.e_ident[..header::SELFMAG];
    let e_type = ehdr.e_type;
    let ei_class = ehdr.e_ident[header::EI_CLASS];
    let ei_data = ehdr.e_ident[header::EI_DATA];
    let ei_version = ehdr.e_ident[header::EI_VERSION];
    magic == header::ELFMAG
        && (e_type == header::ET_EXEC || e_type == header::ET_DYN)
        && ei_class == header::ELFCLASS64
        && ei_data == header::ELFDATA2LSB
        && ei_version == header::EV_CURRENT
}

pub async fn load(
    stopped_task: &mut StoppedTask<'_, '_>,
    exec: Exec,
    file: ExecFile,
) -> Result<(), Errno> {
    let mut tr = Trampoline::new(stopped_task);
    let mut pad = Scratchpad::new(&mut tr).await?;
    let elf_file = ElfFile::from_local(&mut pad, file).await;
    let pad_cleanup_result = pad.free().await;
    let elf_file = elf_file?;
    pad_cleanup_result?;

    let entry = elf_file.load(&mut tr, exec).await;
    let elf_cleanup_result = elf_file.free(&mut tr).await;
    let entry = entry?;
    elf_cleanup_result?;

    entry.init_task(stopped_task);
    Ok(())
}

fn elf_header(header: &FileHeader) -> &Header {
    plain::from_bytes(&header.bytes).unwrap()
}

fn elf_segment(header: &ProgramHeader) -> Result<Segment, Errno> {
    let vaddr = VPtr(header.p_vaddr as usize);
    let file_start = header.p_offset as usize;
    if page_offset(file_start) != VPage::offset(vaddr) {
        return Err(Errno(-abi::EINVAL));
    }
    Ok(Segment {
        mapped_range: MappedRange {
            file_start,
            mem: vaddr..(vaddr + (header.p_filesz as usize)),
        },
        mem_size: (header.p_memsz.max(header.p_filesz) - header.p_filesz) as usize,
        protect: MemProtect {
            read: 0 != (header.p_flags & program_header::PF_R),
            write: 0 != (header.p_flags & program_header::PF_W),
            execute: 0 != (header.p_flags & program_header::PF_X),
        },
    })
}

#[derive(Debug)]
struct ElfAux {
    phdr: VPtr,
    phnum: usize,
    base: VPtr,
    entry: VPtr,
    uid: usize,
    euid: usize,
    gid: usize,
    egid: usize,
}

#[derive(Debug)]
struct ElfEntry {
    ip: VPtr,
    sp: VPtr,
    brk_base: VPage,
}

impl ElfEntry {
    fn init_task(&self, stopped_task: &mut StoppedTask) {
        assert_eq!(self.sp.0 & abi::ELF_STACK_ALIGN_MASK, 0);
        let prev_regs = stopped_task.regs.clone();
        stopped_task.regs.clone_from(&UserRegs {
            sp: self.sp.0,
            ip: self.ip.0,
            cs: prev_regs.cs,
            ss: prev_regs.ss,
            ds: prev_regs.ds,
            es: prev_regs.ds,
            flags: prev_regs.flags,
            ..Default::default()
        });

        stopped_task.task.task_data.mm.randomize_brk(self.brk_base);
    }
}

#[derive(Debug)]
struct ElfFile {
    local: ExecFile,
    remote: TempRemoteFd,
}

impl ElfFile {
    async fn from_local(
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        local: ExecFile,
    ) -> Result<ElfFile, Errno> {
        let remote = TempRemoteFd(RemoteFd::from_local(scratchpad, &local.inner.0.fd).await?);
        Ok(ElfFile { local, remote })
    }

    async fn free(self, trampoline: &mut Trampoline<'_, '_, '_>) -> Result<(), Errno> {
        self.remote.free(trampoline).await
    }

    fn header(&self) -> &Header {
        elf_header(&self.local.header)
    }

    fn program_header_range(&self) -> Range<u16> {
        0..self.header().e_phnum
    }

    fn program_header(&self, idx: u16) -> Result<ProgramHeader, Errno> {
        let ehdr = self.header();
        let mut header = Default::default();
        let bytes = unsafe { plain::as_mut_bytes(&mut header) };
        self.local.inner.0.pread_exact(
            bytes,
            ehdr.e_phoff as usize + ehdr.e_phentsize as usize * idx as usize,
        )?;
        Ok(header)
    }

    async fn load(
        &self,
        trampoline: &mut Trampoline<'_, '_, '_>,
        exec: Exec,
    ) -> Result<ElfEntry, Errno> {
        let offset = self.determine_load_offset();
        let header = self.header();
        let header_ptr = self.locate_header()?;
        let elf_aux = ElfAux {
            phdr: header_ptr + header.e_phoff as usize + offset,
            phnum: header.e_phnum as usize,
            base: header_ptr + offset,
            entry: VPtr(header.e_entry as usize) + offset,
            uid: 0,  // todo
            euid: 0, // todo
            gid: 0,  // todo
            egid: 0, // todo
        };

        let mut pad = Scratchpad::new(trampoline).await?;
        let stack = self.prepare_stack(&mut pad, exec, elf_aux).await;
        let cleanup_result = pad.free().await;
        let stack = stack?;
        cleanup_result?;

        trampoline.unmap_all_userspace_mem().await;

        let stack = stack.load(trampoline).await?;
        self.load_segments(trampoline, offset, &stack).await
    }

    fn determine_load_offset(&self) -> usize {
        if self.header().e_type == header::ET_DYN {
            let rnd_mask = ((1 << abi::MMAP_RND_BITS) - 1) & !(abi::PAGE_SIZE - 1);
            abi::TASK_UNMAPPED_BASE + (nolibc::getrandom_usize() & rnd_mask)
        } else {
            0
        }
    }

    fn locate_header(&self) -> Result<VPtr, Errno> {
        let mut addr = 0;
        // Same technique linux's elf loader uses: vaddr - offset for the first LOAD
        for idx in self.program_header_range() {
            let phdr = self.program_header(idx)?;
            if phdr.p_type == program_header::PT_LOAD {
                addr = (phdr.p_vaddr - phdr.p_offset) as usize;
                break;
            }
        }
        Ok(VPtr(addr))
    }

    fn interp_segment(&self) -> Result<Option<Segment>, Errno> {
        for idx in self.program_header_range() {
            let phdr = self.program_header(idx)?;
            if phdr.p_type == program_header::PT_INTERP {
                return Ok(Some(elf_segment(&phdr)?));
            }
        }
        Ok(None)
    }

    async fn load_segments(
        &self,
        trampoline: &mut Trampoline<'_, '_, '_>,
        offset: usize,
        stack: &InitialStack,
    ) -> Result<ElfEntry, Errno> {
        let header = self.header();
        let mut brk_base = VPage::null();
        for idx in self.program_header_range() {
            let phdr = self.program_header(idx)?;
            if phdr.p_type == program_header::PT_LOAD {
                let segment = elf_segment(&phdr)?;
                let loaded = LoadedSegment::new(
                    trampoline,
                    &self.remote.0,
                    &segment,
                    &MapLocation::Offset(offset),
                )
                .await?;
                brk_base = brk_base.max(loaded.segment().mem_pages().end);
            }
        }
        Ok(ElfEntry {
            ip: VPtr(header.e_entry as usize) + offset,
            sp: stack.sp,
            brk_base,
        })
    }

    async fn prepare_stack(
        &self,
        scratchpad: &mut Scratchpad<'_, '_, '_, '_>,
        exec: Exec,
        elf_aux: ElfAux,
    ) -> Result<StackBuilder, Errno> {
        let mut stack = StackBuilder::new(scratchpad).await?;
        let mut argc = 0;

        let elf_hwcap = raw_cpuid::cpuid!(1).edx as usize;
        let random_data_ptr = stack.push_random_bytes(scratchpad, 16).await?;
        let platform_str_ptr = stack
            .push_bytes(scratchpad, abi::PLATFORM_NAME_BYTES)
            .await?;

        let filename_str =
            VStringRange::parse(&mut scratchpad.trampoline.stopped_task, exec.filename)?;
        let filename_ptr = stack
            .push_remote_bytes(&mut scratchpad.trampoline, filename_str.range())
            .await?;

        for idx in 0.. {
            if let Some(item) = exec
                .argv
                .item_range(&mut scratchpad.trampoline.stopped_task, idx)?
            {
                let argvec = stack
                    .push_remote_bytes(&mut scratchpad.trampoline, item.range())
                    .await?;
                stack.store_vectors(scratchpad, &[argvec.0]).await?;
                argc += 1;
            } else {
                break;
            }
        }
        stack.store_vectors(scratchpad, &[0]).await?;

        for idx in 0.. {
            if let Some(item) = exec
                .envp
                .item_range(&mut scratchpad.trampoline.stopped_task, idx)?
            {
                let envvec = stack
                    .push_remote_bytes(&mut scratchpad.trampoline, item.range())
                    .await?;
                stack.store_vectors(scratchpad, &[envvec.0]).await?;
            } else {
                break;
            }
        }

        // ld.so can show you the aux vectors:
        // cargo run -- -e LD_SHOW_AUXV -- ubuntu /usr/lib/x86_64-linux-gnu/ld-2.31.so
        stack
            .store_vectors(
                scratchpad,
                &[
                    0, // end of envp
                    abi::AT_SYSINFO_EHDR,
                    scratchpad
                        .trampoline
                        .kernel_mem
                        .vdso
                        .pages
                        .mem_pages()
                        .start
                        .ptr()
                        .0,
                    abi::AT_HWCAP,
                    elf_hwcap,
                    abi::AT_PAGESZ,
                    abi::PAGE_SIZE,
                    abi::AT_CLKTCK,
                    abi::USER_HZ,
                    abi::AT_PHDR,
                    elf_aux.phdr.0,
                    abi::AT_PHENT,
                    size_of::<ProgramHeader>(),
                    abi::AT_PHNUM,
                    elf_aux.phnum,
                    abi::AT_BASE,
                    elf_aux.base.0,
                    abi::AT_FLAGS,
                    0,
                    abi::AT_ENTRY,
                    elf_aux.entry.0,
                    abi::AT_UID,
                    elf_aux.uid,
                    abi::AT_EUID,
                    elf_aux.euid,
                    abi::AT_GID,
                    elf_aux.gid,
                    abi::AT_EGID,
                    elf_aux.egid,
                    abi::AT_SECURE,
                    0,
                    abi::AT_RANDOM,
                    random_data_ptr.0,
                    abi::AT_HWCAP2,
                    0,
                    abi::AT_EXECFN,
                    filename_ptr.0,
                    abi::AT_PLATFORM,
                    platform_str_ptr.0,
                    abi::AT_NULL,
                    abi::AT_NULL,
                    abi::AT_NULL,
                    abi::AT_NULL,
                ],
            )
            .await?;

        // argc goes at the lowest stack address, but we don't know it until we've
        // prepared the vectors above it.
        let argc_vec: [usize; 1] = [argc];

        stack.align(abi::ELF_STACK_ALIGN);
        let total_vector_len = stack.stored_vector_bytes() + size_of_val(&argc_vec);
        if 0 != (total_vector_len & abi::ELF_STACK_ALIGN_MASK) {
            let padding = abi::ELF_STACK_ALIGN - (total_vector_len & abi::ELF_STACK_ALIGN_MASK);
            stack.skip_bytes(padding)?;
        }

        stack.push_stored_vectors(scratchpad).await?;

        stack.store_vectors(scratchpad, &argc_vec).await?;
        stack.push_stored_vectors(scratchpad).await?;

        Ok(stack)
    }
}
