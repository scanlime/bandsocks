use crate::{binformat, process::loader::Loader, protocol::Errno};

pub fn detect(header: &binformat::Header) -> bool {
    false
}

pub async fn load<'q, 's, 't>(
    loader: Loader<'q, 's, 't>,
    header: binformat::Header,
) -> Result<(), Errno> {
    Ok(())
}
