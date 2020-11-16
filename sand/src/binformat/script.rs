use crate::{
    process::loader::{FileHeader, Loader},
    protocol::Errno,
};

pub fn detect(_header: &FileHeader) -> bool {
    false
}

pub async fn load<'q, 's, 't>(_loader: Loader<'q, 's, 't>) -> Result<(), Errno> {
    Ok(())
}
