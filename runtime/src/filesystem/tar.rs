// This code may not be used for any purpose. Be gay, do crime.

use crate::filesystem::vfs::{Filesystem, Stat};
use crate::filesystem::mmap::MapRef;
use crate::errors::ImageError;
use tar::{Archive, Entry, EntryType};
use std::io::{Read, Cursor};
use std::sync::Arc;
use memmap::Mmap;

pub fn extract_metadata(mut fs: &mut Filesystem, archive: &Arc<Mmap>) -> Result<(), ImageError> {
    let mut offset: usize = 0;
    while let Some(entry) = Archive::new(Cursor::new(&archive[offset..])).entries()?.next() {
        let entry = entry?;
        let file_begin = offset + (entry.raw_file_position() as usize);
        let file = MapRef::new(archive, file_begin, entry.size() as usize);
        offset = pad_to_block_multiple(file_begin + entry.size() as usize);
        extract_file_metadata(&mut fs, entry, file)?;
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

fn extract_file_metadata<'a, R: Read> (fs: &mut Filesystem, entry: Entry<'a, R>, file: MapRef) -> Result<(), ImageError> {
    let mut fsw = fs.writer();
    let kind = entry.header().entry_type();
    let path = entry.path()?;
    let link_name = entry.link_name()?;
    let stat = Stat {
        mode: entry.header().mode()?,
        uid: entry.header().uid()?,
        gid: entry.header().gid()?,
        mtime: entry.header().mtime()?,
        ..Default::default()
    };
            
    match kind {
        EntryType::Regular => fsw.write_file_mapping(&path, file, stat)?,
        EntryType::Directory => fsw.write_directory_metadata(&path, stat)?,
        EntryType::Symlink => match link_name {
            Some(link_name) => fsw.write_symlink(&path, &link_name, stat)?,
            None => Err(ImageError::TARFileError)?,
        },
        EntryType::Link => match link_name {
            Some(link_name) => fsw.write_hardlink(&path, &link_name)?,
            None => Err(ImageError::TARFileError)?,
        },
        _ => log::error!("skipping unsupported tar file entry type {:?}, {:?}", kind, entry.header()),
    }

    Ok(())
}
