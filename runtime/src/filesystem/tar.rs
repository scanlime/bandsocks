// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::vfs::Filesystem;
use crate::filesystem::mmap::MapRef;
use crate::errors::ImageError;
use basic_tar::{U64Ext, BasicTarError, Header, raw};
use std::io::{Read, Seek, Cursor, SeekFrom};
use std::sync::Arc;
use std::convert::TryInto;
use memmap::Mmap;

pub fn extract_metadata(fs: &mut Filesystem, archive: &Arc<Mmap>) -> Result<(), ImageError> {
    let mut cursor = Cursor::new(&archive[..]);
    while (cursor.position() as usize) < archive.len() {
        
        let mut header_raw = raw::header::raw();
        cursor.read_exact(&mut header_raw)?;
        
        match Header::parse(header_raw) {
            Err(BasicTarError::EmptyHeader) => {
                // Ignored; this is normal at the end of the archive
            },
            Err(e) => Err(e)?,
            Ok(header) => {
                let payload = MapRef::new(archive, cursor.position().try_into()?, header.size.try_into()?);
                cursor.seek(SeekFrom::Current(header.size.ceil_to_multiple_of(raw::BLOCK_LEN as u64) as i64))?;
                extract_file_metadata(fs, header, payload);
            }
        }
    }
    Ok(())
}


pub fn extract_file_metadata(fs: &mut Filesystem, header: Header, payload: MapRef) -> Result<(), ImageError> {
    log::info!("{} {:?}", header.path, payload);
    Ok(())
}
