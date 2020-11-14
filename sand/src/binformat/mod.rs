pub mod elf64;
pub mod header;
pub mod script;

use crate::{abi, binformat::header::Header, process::loader::Loader, protocol::Errno};

pub async fn exec<'q, 's, 't>(loader: Loader<'q, 's, 't>, header: Header) -> Result<(), Errno> {
    if script::detect(&header) {
        script::load(loader, header).await
    } else if elf64::detect(&header) {
        elf64::load(loader, header).await
    } else {
        Err(Errno(-abi::ENOEXEC as i32))
    }
}
