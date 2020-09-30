// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::vfs::{Filesystem, Stat};
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
        let file = MapRef::new(archive, file_begin, entry.size() as usize);
        offset = pad_to_block_multiple(file_begin + entry.size() as usize);
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
    let mut fsw = fs.writer();
    let path = header.path()?;
    let stat = Stat {
        mode: header.mode()?,
        uid: header.uid()?,
        gid: header.gid()?,
        mtime: header.mtime()?,
        ..Default::default()
    };
            
    match header.entry_type() {
        EntryType::Regular => fsw.write_file_mapping(&path, file, stat)?,
        EntryType::Directory => fsw.write_directory_metadata(&path, stat)?,
        EntryType::Symlink => {
        },
        EntryType::Link => {
        },
        EntryType::Char => {
        },
        EntryType::Block => {
        },
        EntryType::Fifo => {
        },
        _ => log::error!("skipping unsupported tar file entry type, {:?}", header),
    }

    Ok(())
}
