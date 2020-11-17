use crate::{
    abi,
    abi::UserRegs,
    process::loader::{FileHeader, Loader},
    protocol::{Errno, VPtr},
};
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
    &ehdr.e_ident[..header::SELFMAG] == header::ELFMAG
        && ehdr.e_ident[header::EI_CLASS] == header::ELFCLASS64
        && ehdr.e_ident[header::EI_DATA] == header::ELFDATA2LSB
        && ehdr.e_ident[header::EI_VERSION] == header::EV_CURRENT
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
    println!("ELF64 {:?}", ehdr);

    let mut stack = loader.stack_begin().await?;

    for idx in 0.. {
        if let Some(env) = loader.envp_read(idx)? {
            println!("env {:?} {:x?} {:?}", idx, env, loader.vstring_len(env));
        } else {
            break;
        }
    }

    for idx in 0.. {
        if let Some(arg) = loader.argv_read(idx)? {
            println!("arg {:?} {:x?} {:?}", idx, arg, loader.vstring_len(arg));
        } else {
            break;
        }
    }

    // loader
    //     .stack_remote_bytes(&mut stack, loader.argv, 16)
    //     .await?;
    stack.align(16);
    loader.stack_bytes(&mut stack, &[1, 2, 3]).await?;
    stack.align(16);

    let prev_regs = loader.userspace_regs().clone();
    loader.userspace_regs().clone_from(&UserRegs {
        sp: stack.stack_bottom().0 as u64,
        ip: ehdr.e_entry,
        cs: prev_regs.cs,
        ss: prev_regs.ss,
        ds: prev_regs.ds,
        es: prev_regs.es,
        fs: prev_regs.fs,
        gs: prev_regs.gs,
        flags: prev_regs.flags,
        ..Default::default()
    });

    println!("stack, {:x?}", stack);
    loader.unmap_all_userspace_mem().await;
    loader.stack_finish(stack).await?;

    let mut brk = VPtr(0);

    for idx in 0..ehdr.e_phnum {
        let phdr = elf64_program_header(&loader, &ehdr, idx)?;
        if phdr.p_type == program_header::PT_LOAD
            && abi::page_offset(phdr.p_offset as usize) == abi::page_offset(phdr.p_vaddr as usize)
        {
            let prot = phdr_prot(&phdr);
            let page_alignment = abi::page_offset(phdr.p_vaddr as usize);
            let start_ptr = VPtr(phdr.p_vaddr as usize - page_alignment);
            let file_size_aligned = abi::page_round_up(phdr.p_filesz as usize + page_alignment);
            let file_offset_aligned = phdr.p_offset as usize - page_alignment;
            let mem_size_aligned = abi::page_round_up(phdr.p_memsz as usize + page_alignment);

            if phdr.p_memsz > phdr.p_filesz {
                loader
                    .map_anonymous(start_ptr, mem_size_aligned, prot)
                    .await
                    .expect("loader map_anonymous failed");
            }

            if phdr.p_filesz > 0 {
                loader
                    .map_file(start_ptr, file_size_aligned, file_offset_aligned, prot)
                    .await
                    .expect("loader map_file failed");
            }

            brk = brk.max(start_ptr.add(mem_size_aligned));
        }
    }

    loader.randomize_brk(brk);
    Ok(())
}
