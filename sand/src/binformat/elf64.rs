use crate::{
    abi, nolibc,
    process::loader::{FileHeader, Loader},
    protocol::{abi::UserRegs, Errno, VPtr},
};
use core::mem::{size_of, size_of_val};
use goblin::elf64::{header, header::Header, program_header, program_header::ProgramHeader};

fn elf64_header(fh: &FileHeader) -> Header {
    *plain::from_bytes(&fh.bytes).unwrap()
}

fn elf64_program_header(loader: &Loader, ehdr: &Header, idx: u16) -> Result<ProgramHeader, Errno> {
    let mut header = Default::default();
    let bytes = unsafe { plain::as_mut_bytes(&mut header) };
    loader.read_file(
        ehdr.e_phoff as usize + ehdr.e_phentsize as usize * idx as usize,
        bytes,
    )?;
    Ok(header)
}

struct LoadAddr {
    ehdr_addr: VPtr,
    interp_addr: VPtr,
    load_offset: usize,
}

impl LoadAddr {
    fn new(loader: &Loader, ehdr: &Header) -> Result<LoadAddr, Errno> {
        let load_offset = LoadAddr::determine_load_offset(ehdr);
        let ehdr_addr = LoadAddr::determine_ehdr_addr(loader, ehdr)?.add(load_offset);
        let interp_addr = ehdr_addr;
        Ok(LoadAddr {
            ehdr_addr,
            interp_addr,
            load_offset,
        })
    }

    fn determine_ehdr_addr(loader: &Loader, ehdr: &Header) -> Result<VPtr, Errno> {
        let mut addr = 0;
        // Same technique linux's elf loader uses: vaddr - offset for the first LOAD
        for idx in 0..ehdr.e_phnum {
            let phdr = elf64_program_header(&loader, &ehdr, idx)?;
            if phdr.p_type == program_header::PT_LOAD {
                addr = (phdr.p_vaddr - phdr.p_offset) as usize;
                break;
            }
        }
        Ok(VPtr(addr))
    }

    fn determine_load_offset(ehdr: &Header) -> usize {
        if ehdr.e_type == header::ET_DYN {
            let rnd = nolibc::getrandom_usize();
            let rnd_mask = ((1 << abi::MMAP_RND_BITS) - 1) & !abi::PAGE_MASK;
            abi::TASK_UNMAPPED_BASE + (rnd & rnd_mask)
        } else {
            0
        }
    }
}

pub fn detect(fh: &FileHeader) -> bool {
    let ehdr = elf64_header(fh);
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

fn phdr_prot(phdr: &ProgramHeader) -> isize {
    let mut prot = 0;
    if 0 != (phdr.p_flags & program_header::PF_R) {
        prot |= abi::PROT_READ
    }
    if 0 != (phdr.p_flags & program_header::PF_W) {
        prot |= abi::PROT_WRITE
    }
    if 0 != (phdr.p_flags & program_header::PF_X) {
        prot |= abi::PROT_EXEC
    }
    prot
}

pub async fn load<'q, 's, 't>(mut loader: Loader<'q, 's, 't>) -> Result<(), Errno> {
    let ehdr = elf64_header(loader.file_header());
    let lad = LoadAddr::new(&loader, &ehdr)?;
    let stack_ptr = replace_maps_with_new_stack(&mut loader, &ehdr, &lad).await?;
    load_segments(&mut loader, &ehdr, &lad).await?;
    init_registers(&mut loader, &ehdr, &lad, stack_ptr);
    Ok(())
}

fn init_registers(loader: &mut Loader<'_, '_, '_>, ehdr: &Header, lad: &LoadAddr, stack_ptr: VPtr) {
    let prev_regs = loader.userspace_regs().clone();
    loader.userspace_regs().clone_from(&UserRegs {
        sp: stack_ptr.0,
        ip: ehdr.e_entry as usize + lad.load_offset,
        cs: prev_regs.cs,
        ss: prev_regs.ss,
        ds: prev_regs.ds,
        es: prev_regs.ds,
        flags: prev_regs.flags,
        ..Default::default()
    });
}

async fn replace_maps_with_new_stack(
    loader: &mut Loader<'_, '_, '_>,
    ehdr: &Header,
    lad: &LoadAddr,
) -> Result<VPtr, Errno> {
    let mut stack = loader.stack_begin().await?;
    let mut argc = 0;

    let elf_hwcap = raw_cpuid::cpuid!(1).edx as usize;
    let random_data_ptr = loader.stack_random_bytes(&mut stack, 16).await?;
    let platform_str_ptr = loader
        .stack_bytes(&mut stack, abi::PLATFORM_NAME_BYTES)
        .await?;

    let filename_len = 1 + loader.vstring_len(loader.filename())?;
    let filename_ptr = loader
        .stack_remote_bytes(&mut stack, loader.filename().0, filename_len)
        .await?;

    for idx in 0.. {
        if let Some(arg) = loader.argv_read(idx)? {
            let length = 1 + loader.vstring_len(arg)?;
            let argvec = loader.stack_remote_bytes(&mut stack, arg.0, length).await?;
            loader.store_vectors(&mut stack, &[argvec.0]).await?;
            argc += 1;
        } else {
            break;
        }
    }
    loader.store_vectors(&mut stack, &[0]).await?;

    for idx in 0.. {
        if let Some(env) = loader.envp_read(idx)? {
            let length = 1 + loader.vstring_len(env)?;
            let envvec = loader.stack_remote_bytes(&mut stack, env.0, length).await?;
            loader.store_vectors(&mut stack, &[envvec.0]).await?;
        } else {
            break;
        }
    }

    // ld.so can show you the aux vectors:
    // cargo run -- -e LD_SHOW_AUXV -- ubuntu /usr/lib/x86_64-linux-gnu/ld-2.31.so
    loader
        .store_vectors(
            &mut stack,
            &[
                0, // end of envp
                abi::AT_SYSINFO_EHDR,
                loader.vdso().start,
                abi::AT_HWCAP,
                elf_hwcap,
                abi::AT_PAGESZ,
                abi::PAGE_SIZE,
                abi::AT_CLKTCK,
                abi::USER_HZ,
                abi::AT_PHDR,
                lad.ehdr_addr.add(ehdr.e_phoff as usize).0,
                abi::AT_PHENT,
                size_of::<ProgramHeader>(),
                abi::AT_PHNUM,
                ehdr.e_phnum as usize,
                abi::AT_BASE,
                lad.interp_addr.0,
                abi::AT_FLAGS,
                0,
                abi::AT_ENTRY,
                lad.load_offset + ehdr.e_entry as usize,
                abi::AT_UID,
                0, //to do
                abi::AT_EUID,
                0, //to do
                abi::AT_GID,
                0, //to do
                abi::AT_EGID,
                0, //to do
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

    loader.stack_stored_vectors(&mut stack).await?;
    loader.store_vectors(&mut stack, &argc_vec).await?;
    let sp = loader.stack_stored_vectors(&mut stack).await?;
    loader.unmap_all_userspace_mem().await;
    loader.stack_finish(stack).await?;

    assert_eq!(sp.0 & abi::ELF_STACK_ALIGN_MASK, 0);
    Ok(sp)
}

async fn load_segments(
    loader: &mut Loader<'_, '_, '_>,
    ehdr: &Header,
    lad: &LoadAddr,
) -> Result<VPtr, Errno> {
    let mut brk = VPtr(0);

    for idx in 0..ehdr.e_phnum {
        let phdr = elf64_program_header(&loader, &ehdr, idx)?;
        if phdr.p_type == program_header::PT_LOAD
            && abi::page_offset(phdr.p_offset as usize) == abi::page_offset(phdr.p_vaddr as usize)
        {
            let prot = phdr_prot(&phdr);
            let page_alignment = abi::page_offset(phdr.p_vaddr as usize);
            let start_ptr = VPtr(phdr.p_vaddr as usize - page_alignment + lad.load_offset);
            let file_size_aligned = abi::page_round_up(phdr.p_filesz as usize + page_alignment);
            let file_offset_aligned = phdr.p_offset as usize - page_alignment;
            let mem_size_aligned = abi::page_round_up(phdr.p_memsz as usize + page_alignment);

            if phdr.p_memsz > phdr.p_filesz {
                loader
                    .map_anonymous(start_ptr, mem_size_aligned, prot)
                    .await?;
                // FIXME: also need to zero the non-page-aligned portion of the
                // bss
            }
            if phdr.p_filesz > 0 {
                loader
                    .map_file(start_ptr, file_size_aligned, file_offset_aligned, prot)
                    .await?;
            }

            brk = brk.max(start_ptr.add(mem_size_aligned));
        }
    }

    loader.randomize_brk(brk);
    Ok(VPtr(ehdr.e_entry as usize).add(lad.load_offset))
}
