use crate::{binformat, process::loader::Loader, protocol::Errno};
use goblin::elf64;

fn elf64_header(header: &binformat::Header) -> &elf64::header::Header {
    plain::from_bytes(&header.bytes).unwrap()
}

fn program_header(
    loader: &Loader<'_, '_, '_>,
    ehdr: &elf64::header::Header,
    idx: u16,
) -> Result<elf64::program_header::ProgramHeader, Errno> {
    let mut header = Default::default();
    let mut bytes = unsafe { plain::as_mut_bytes(&mut header) };
    loader.read(
        ehdr.e_phoff as usize + ehdr.e_phentsize as usize * idx as usize,
        bytes,
    )?;
    Ok(header)
}

pub fn detect(header: &binformat::Header) -> bool {
    let ehdr = elf64_header(header);
    &ehdr.e_ident[..elf64::header::SELFMAG] == elf64::header::ELFMAG
        && ehdr.e_ident[elf64::header::EI_CLASS] == elf64::header::ELFCLASS64
        && ehdr.e_ident[elf64::header::EI_DATA] == elf64::header::ELFDATA2LSB
        && ehdr.e_ident[elf64::header::EI_VERSION] == elf64::header::EV_CURRENT
}

pub async fn load<'q, 's, 't>(
    loader: Loader<'q, 's, 't>,
    header: binformat::Header,
) -> Result<(), Errno> {
    let ehdr = elf64_header(&header);
    println!("{:?}", ehdr);
    for idx in 0..ehdr.e_phnum {
        let phdr = program_header(&loader, &ehdr, idx)?;
        println!("{:?}", phdr.vm_range());
    }
    Ok(())
}
