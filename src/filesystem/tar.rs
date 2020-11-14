use crate::{
    errors::ImageError,
    filesystem::{
        storage::{FileStorage, StorageKey},
        vfs::{Filesystem, Stat},
    },
};
use std::io::{Cursor, Read};
use tar::{Archive, Entry, EntryType};

pub async fn extract(
    fs: &mut Filesystem,
    storage: &FileStorage,
    archive: &StorageKey,
) -> Result<(), ImageError> {
    let mut offset: usize = 0;
    let archive_map = match storage.get(archive).await? {
        Some(mapref) => mapref,
        None => return Err(ImageError::TARFileError),
    };
    while let Some(entry) = Archive::new(Cursor::new(&archive_map[offset..]))
        .entries()?
        .next()
    {
        let entry = entry?;
        let entry_size = entry.size() as usize;
        let file_begin = offset + (entry.raw_file_position() as usize);
        let file_range = file_begin..(file_begin + entry_size);
        let file_key = if entry_size == 0 {
            None
        } else {
            Some(
                archive
                    .range(file_range)
                    .map_err(|_| ImageError::TARFileError)?,
            )
        };
        offset = pad_to_block_multiple(file_begin + entry_size);
        extract_file_metadata(fs, entry, file_key)?;
    }
    Ok(())
}

fn pad_to_block_multiple(size: usize) -> usize {
    const BLOCK_LEN: usize = 512;
    let rem = size % BLOCK_LEN;
    if rem == 0 {
        size
    } else {
        size + (BLOCK_LEN - rem)
    }
}

fn extract_file_metadata<'a, R: Read>(
    fs: &mut Filesystem,
    entry: Entry<'a, R>,
    data: Option<StorageKey>,
) -> Result<(), ImageError> {
    let mut fsw = fs.writer();
    let kind = entry.header().entry_type();
    let path = entry.path()?;
    let link_name = entry.link_name()?;
    let stat = Stat {
        nlink: 0,
        mode: entry.header().mode()?,
        uid: entry.header().uid()?,
        gid: entry.header().gid()?,
        mtime: entry.header().mtime()?,
        size: entry.header().size()?,
    };

    match kind {
        EntryType::Regular => fsw.write_file(&path, data, stat)?,
        EntryType::Directory => fsw.write_directory_metadata(&path, stat)?,
        EntryType::Symlink => match link_name {
            Some(link_name) => fsw.write_symlink(&path, &link_name, stat)?,
            None => Err(ImageError::TARFileError)?,
        },
        EntryType::Link => match link_name {
            Some(link_name) => fsw.write_hardlink(&path, &link_name)?,
            None => Err(ImageError::TARFileError)?,
        },
        _ => log::error!(
            "skipping unsupported tar file entry type {:?}, {:?}",
            kind,
            entry.header()
        ),
    }

    Ok(())
}
