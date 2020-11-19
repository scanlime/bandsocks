use crate::{
    abi,
    abi::UserRegs,
    process::loader::{FileHeader, Loader},
    protocol::{Errno, VPtr},
};
use core::mem::size_of;
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
    let load_base = VPtr(match ehdr.e_type {
        header::ET_EXEC => 0,
        header::ET_DYN => {
            // to do: calculate this at a more suitable location, and randomize it
            0x1_0000_0000
        }
        _ => unreachable!(),
    });

    let stack_ptr = replace_maps_with_new_stack(&mut loader, load_base, &ehdr).await?;
    load_segments(&mut loader, load_base, &ehdr).await?;
    init_registers(&mut loader, stack_ptr, load_base, &ehdr);
    Ok(())
}

fn init_registers(
    loader: &mut Loader<'_, '_, '_>,
    stack_ptr: VPtr,
    load_base: VPtr,
    ehdr: &Header,
) {
    let prev_regs = loader.userspace_regs().clone();
    loader.userspace_regs().clone_from(&UserRegs {
        sp: stack_ptr.0 as u64,
        ip: ehdr.e_entry + load_base.0 as u64,
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
    load_base: VPtr,
    ehdr: &Header,
) -> Result<VPtr, Errno> {
    let elf_hwcap = raw_cpuid::cpuid!(1).edx as usize;
    let mut stack = loader.stack_begin().await?;
    let mut argc = 0;

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
                load_base.0 + ehdr.e_phoff as usize,
                abi::AT_PHENT,
                size_of::<ProgramHeader>(),
                abi::AT_PHNUM,
                ehdr.e_phnum as usize,
                abi::AT_BASE,
                load_base.0,
                abi::AT_FLAGS,
                0,
                abi::AT_ENTRY,
                load_base.0 + ehdr.e_entry as usize,
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
                abi::AT_HWCAP2,
                0,
                abi::AT_EXECFN,
                filename_ptr.0,
                abi::AT_NULL,
                abi::AT_NULL,
                abi::AT_NULL,
                abi::AT_NULL,
            ],
        )
        .await?;

    stack.align(16);
    loader.stack_stored_vectors(&mut stack).await?;
    loader.store_vectors(&mut stack, &[argc]).await?;
    let sp = loader.stack_stored_vectors(&mut stack).await?;
    loader.unmap_all_userspace_mem().await;
    loader.stack_finish(stack).await?;
    Ok(sp)
}

async fn load_segments(
    loader: &mut Loader<'_, '_, '_>,
    load_base: VPtr,
    ehdr: &Header,
) -> Result<VPtr, Errno> {
    let mut brk = VPtr(0);

    for idx in 0..ehdr.e_phnum {
        let phdr = elf64_program_header(&loader, &ehdr, idx)?;
        if phdr.p_type == program_header::PT_LOAD
            && abi::page_offset(phdr.p_offset as usize) == abi::page_offset(phdr.p_vaddr as usize)
        {
            let prot = phdr_prot(&phdr);
            let page_alignment = abi::page_offset(phdr.p_vaddr as usize);
            let start_ptr = VPtr(phdr.p_vaddr as usize - page_alignment + load_base.0);
            let file_size_aligned = abi::page_round_up(phdr.p_filesz as usize + page_alignment);
            let file_offset_aligned = phdr.p_offset as usize - page_alignment;
            let mem_size_aligned = abi::page_round_up(phdr.p_memsz as usize + page_alignment);

            if phdr.p_memsz > phdr.p_filesz {
                loader
                    .map_anonymous(start_ptr, mem_size_aligned, prot)
                    .await?;
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
    Ok(load_base.add(ehdr.e_entry as usize))
}
