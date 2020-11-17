pub mod elf64;
pub mod script;

use crate::{abi, process::loader::Loader, protocol::Errno};

pub async fn exec<'q, 's, 't>(loader: Loader<'q, 's, 't>) -> Result<(), Errno> {
    if script::detect(loader.file_header()) {
        script::load(loader).await
    } else if elf64::detect(loader.file_header()) {
        elf64::load(loader).await
    } else {
        Err(Errno(-abi::ENOEXEC))
    }
}
