// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::vfs::Filesystem;
use crate::filesystem::mmap::MapRef;
use crate::errors::ImageError;
use tar::{Archive, Header, EntryType};
use std::io::Cursor;
use std::sync::Arc;
use memmap::Mmap;

pub fn extract_metadata(mut fs: &mut Filesystem, archive: &Arc<Mmap>) -> Result<(), ImageError> {
    let mut offset: usize = 0;
    while let Some(entry) = Archive::new(Cursor::new(&archive[offset..])).entries()?.next() {
        let entry = entry?;
        let file_begin = offset + (entry.raw_file_position() as usize);
        let file_end = file_begin + (entry.size() as usize);
        let file = MapRef::new(archive, file_begin, file_end);
        offset = pad_to_block_multiple(file_end);
        extract_file_metadata(&mut fs, entry.header(), file);
    }
    Ok(())
}

fn pad_to_block_multiple(size: usize) -> usize{
    const BLOCK_LEN: usize = 512;
    let rem = size % BLOCK_LEN;
    if rem == 0 {
        size
    } else {
        size + (BLOCK_LEN - rem)
    }
}

fn extract_file_metadata(fs: &mut Filesystem, header: &Header, file: MapRef) -> Result<(), ImageError> {

    for path_component in header.path()?.components() {
        println!("{:?}", path_component);
    }
    
    match header.entry_type() {
        EntryType::Regular => {
            
        },
        EntryType::Directory => {
        },
        EntryType::Symlink => {
        },
        unknown => {
            log::error!("skipping unsupported tar file entry type, {:?}", header);
        }
    }

    Ok(())
}
