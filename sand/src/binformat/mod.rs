pub mod elf64;
pub mod script;

use crate::{abi, process::loader::Loader, protocol::Errno};

#[derive(Debug)]
#[repr(C)]
#[repr(align(8))]
pub struct Header {
    pub bytes: [u8; abi::BINPRM_BUF_SIZE],
}

pub async fn exec<'q, 's, 't>(loader: Loader<'q, 's, 't>, header: Header) -> Result<(), Errno> {
    if script::detect(&header) {
        script::load(loader, header).await
    } else if elf64::detect(&header) {
        elf64::load(loader, header).await
    } else {
        Err(Errno(-abi::ENOEXEC as i32))
    }
}
