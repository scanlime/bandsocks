use crate::{
    abi,
    binformat::Header,
    process::loader::Loader,
    protocol::{Errno, SysFd},
};

pub fn detect(header: &Header) -> bool {
    false
}

pub async fn load<'q, 's, 't>(loader: Loader<'q, 's, 't>, header: Header) -> Result<(), Errno> {
    Ok(())
}
