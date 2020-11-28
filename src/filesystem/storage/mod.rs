mod key;
mod writer;

pub use key::StorageKey;
pub use writer::StorageWriter;

use crate::errors::ImageError;
use memmap::{Mmap, MmapOptions};
use std::{
    env, fs,
    fs::{File, OpenOptions},
    io,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use tokio::task;

pub fn default_cache_dir() -> Result<PathBuf, ImageError> {
    match env::var("BANDSOCKS_CACHE") {
        Ok(s) => Ok(Path::new(&s).to_path_buf()),
        Err(_) => {
            let mut buf = match env::var("XDG_CACHE_HOME") {
                Ok(s) => Ok(Path::new(&s).to_path_buf()),
                Err(_) => match env::var("HOME") {
                    Ok(s) => Ok(Path::new(&s).join(".cache")),
                    Err(_) => Err(ImageError::NoDefaultCacheDir),
                },
            };
            if let Ok(buf) = &mut buf {
                buf.push("bandsocks");
            }
            buf
        }
    }
}

fn create_parent_dirs(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            // Log a warning instead of giving up right away, in case this was a race
            // condition.
            log::warn!("error creating temp directory at {:?}, {:?}", parent, err);
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileStorage {
    path: PathBuf,
}

impl FileStorage {
    pub fn new(path: PathBuf) -> Self {
        FileStorage { path }
    }

    /// Open one object from local storage, as a File
    pub fn open(&self, key: &StorageKey) -> Result<Option<File>, ImageError> {
        let path = key.to_path(&self.path);
        match File::open(path) {
            Err(e) => match e.kind() {
                io::ErrorKind::NotFound => Ok(None),
                _ => Err(e.into()),
            },
            Ok(f) => Ok(Some(f)),
        }
    }

    /// Open an object, creating requested BlobParts on demand
    pub async fn open_part(&self, key: &StorageKey) -> Result<Option<File>, ImageError> {
        match self.open(key)? {
            Some(f) => Ok(Some(f)),
            None => match key.clone() {
                StorageKey::BlobPart(digest, range) => {
                    let task_storage = self.clone();
                    let task_key = key.clone();
                    task::spawn_blocking(move || {
                        match task_storage.mmap(&StorageKey::Blob(digest))? {
                            None => Ok(None),
                            Some(part_of) => {
                                let mut part = &part_of[range];
                                let mut writer = task_storage.begin_write()?;
                                io::copy(&mut part, &mut writer)?;
                                task_storage.commit_write(writer, &task_key)?;
                                task_storage.open(&task_key)
                            }
                        }
                    })
                    .await?
                }
                _ => Ok(None),
            },
        }
    }

    /// Check whether a stored file exists without actually opening it
    ///
    /// Returns true if and only if the storage exists as a regular file. Any
    /// errors will cause this to return false.
    pub fn exists(&self, key: &StorageKey) -> bool {
        let path = key.to_path(&self.path);
        match fs::metadata(path) {
            Err(_) => false,
            Ok(metadata) => metadata.is_file(),
        }
    }

    /// Make a new storage object at `to_key` using the data from `from_key`
    pub async fn copy_data(
        &self,
        from_key: &StorageKey,
        to_key: &StorageKey,
    ) -> Result<bool, ImageError> {
        let storage = self.clone();
        let from_key = from_key.clone();
        let to_key = to_key.clone();
        task::spawn_blocking(move || {
            let source = storage.mmap(&from_key)?;
            if let Some(source) = source {
                let mut slice = &source[..];
                let mut writer = storage.begin_write()?;
                io::copy(&mut slice, &mut writer)?;
                storage.commit_write(writer, &to_key)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .await?
    }

    /// Open a storage object and memory map it
    pub fn mmap(&self, key: &StorageKey) -> Result<Option<Mmap>, ImageError> {
        match self.open(key)? {
            Some(file) => Ok(Some(unsafe { MmapOptions::new().map(&file) }?)),
            None => Ok(None),
        }
    }

    /// Begin writing to temporary storage
    pub fn begin_write(&self) -> Result<StorageWriter, ImageError> {
        let key = StorageKey::temp();
        let temp_path = key.to_path(&self.path);
        create_parent_dirs(&temp_path);

        let temp_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o440)
            .open(&temp_path)?;

        Ok(StorageWriter::new(key, temp_file, temp_path))
    }

    /// Promote a temporary file into a StorageKey
    pub fn commit_write(
        &self,
        mut writer: StorageWriter,
        key: &StorageKey,
    ) -> Result<(), ImageError> {
        let content_digest = writer.finalize()?;
        let dest_path = key.to_path(&self.path);
        create_parent_dirs(&dest_path);
        writer.rename_temp(&dest_path)?;
        log::debug!("storage commit, {:?} -> {:?}", content_digest, dest_path);
        Ok(())
    }
}
