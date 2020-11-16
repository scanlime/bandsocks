use crate::{
    abi, binformat,
    process::loader::Loader,
    protocol::{Errno, VPtr},
};

pub fn detect(_header: &binformat::Header) -> bool {
    true
}

pub async fn load<'q, 's, 't>(
    mut loader: Loader<'q, 's, 't>,
    _header: binformat::Header,
) -> Result<(), Errno> {
    // experiment
    loader.unmap_all_userspace_mem().await;
    loader
        .mmap(VPtr(0x100000), 0x1000, abi::PROT_READ, abi::MAP_PRIVATE, 0)
        .await?;
    loader.debug_loop().await;
    Ok(())
}
