use crate::{binformat::Header, process::loader::Loader, protocol::Errno};

pub fn detect(_header: &Header) -> bool {
    false
}

pub async fn load<'q, 's, 't>(_loader: Loader<'q, 's, 't>, _header: Header) -> Result<(), Errno> {
    Ok(())
}
