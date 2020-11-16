use crate::{
    abi,
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
    loader.read(
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

pub async fn load<'q, 's, 't>(mut loader: Loader<'q, 's, 't>) -> Result<(), Errno> {
    let ehdr = elf64_header(loader.file_header());
    println!("ELF64 {:?}", ehdr);

    loader.unmap_all_userspace_mem().await;

    for idx in 0..ehdr.e_phnum {
        let phdr = elf64_program_header(&loader, &ehdr, idx)?;
        if phdr.p_type == program_header::PT_LOAD {
            loader
                .mmap(
                    VPtr(phdr.p_vaddr as usize),
                    phdr.p_memsz as usize,
                    abi::PROT_READ,
                    abi::MAP_PRIVATE,
                    phdr.p_offset as usize,
                )
                .await?;

            println!(
                "{:x?}",
                (
                    phdr.p_type,
                    phdr.p_flags,
                    phdr.p_offset,
                    phdr.p_vaddr,
                    phdr.p_paddr,
                    phdr.p_filesz,
                    phdr.p_memsz,
                    phdr.p_align
                )
            );
        }
    }

    loader.debug_loop().await;
    Ok(())
}
