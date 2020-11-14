use crate::{errors::ImageError, Reference};
use memmap::{Mmap, MmapOptions};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    hash::{Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::{
    fs::{os::unix::OpenOptionsExt, File, OpenOptions},
    io::AsyncWriteExt,
};

#[derive(Clone, Debug)]
pub struct FileStorage {
    path: PathBuf,
}

impl FileStorage {
    pub fn new(path: PathBuf) -> Self {
        FileStorage { path }
    }

    pub async fn open(&self, key: &StorageKey) -> Result<Option<File>, ImageError> {
        let path = key.to_path(&self.path)?;
        match File::open(path).await {
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Ok(None),
                _ => Err(e.into()),
            },
            Ok(f) => Ok(Some(f)),
        }
    }

    pub async fn open_part(&self, key: &StorageKey) -> Result<Option<File>, ImageError> {
        match self.open(key).await? {
            Some(f) => Ok(Some(f)),
            None => match key.clone() {
                StorageKey::BlobPart(digest, range) => {
                    match self.mmap(&StorageKey::Blob(digest)).await? {
                        None => Ok(None),
                        Some(part_of) => {
                            let part = &part_of[range];
                            self.insert(key, &part).await?;
                            self.open(key).await
                        }
                    }
                }
                _ => Ok(None),
            },
        }
    }

    pub async fn mmap(&self, key: &StorageKey) -> Result<Option<Mmap>, ImageError> {
        match self.open(key).await? {
            Some(file) => {
                let file = file.into_std().await;
                Ok(Some(unsafe { MmapOptions::new().map(&file) }?))
            }
            None => Ok(None),
        }
    }

    pub async fn insert(&self, key: &StorageKey, data: &[u8]) -> Result<(), ImageError> {
        log::info!("insert {:?}, {} bytes", key, data.len());

        // Prepare directories
        let mut temp_path = self.path.clone();
        push_temp_path(&mut temp_path)?;
        if let Some(parent) = temp_path.as_path().parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let dest_path = key.to_path(&self.path)?;
        if let Some(parent) = dest_path.as_path().parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write data to a nearby temp file first
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o440)
            .open(&temp_path)
            .await?;
        file.write_all(&data).await?;
        file.flush().await?;
        std::mem::drop(file);

        // atomic and idempotent
        tokio::fs::rename(temp_path, dest_path).await?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum StorageKey {
    Blob(String),
    Manifest(Reference),
    BlobPart(String, Range<usize>),
}

impl StorageKey {
    pub fn from_blob_data(data: &[u8]) -> StorageKey {
        StorageKey::Blob(format!("sha256:{:x}", Sha256::digest(data)))
    }

    pub fn range(&self, range: Range<usize>) -> Result<StorageKey, ()> {
        match self {
            StorageKey::Blob(digest) => Ok(StorageKey::BlobPart(digest.clone(), range)),
            StorageKey::Manifest(_) => Err(()),
            StorageKey::BlobPart(digest, part) => Ok(StorageKey::BlobPart(
                digest.clone(),
                (range.start + part.start)..(range.end + part.start),
            )),
        }
    }
}

impl Eq for StorageKey {}

impl PartialEq for StorageKey {
    fn eq(&self, other: &Self) -> bool {
        match self {
            StorageKey::Blob(s) => match other {
                StorageKey::Blob(o) => s == o,
                _ => false,
            },
            StorageKey::BlobPart(s, r) => match other {
                StorageKey::BlobPart(o, q) => s == o && r == q,
                _ => false,
            },
            StorageKey::Manifest(s) => match other {
                StorageKey::Manifest(o) => {
                    s.registry() == o.registry()
                        && s.repository() == o.repository()
                        && s.version() == o.version()
                }
                _ => false,
            },
        }
    }
}

impl Hash for StorageKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            StorageKey::Blob(s) => s.hash(state),
            StorageKey::BlobPart(s, r) => {
                s.hash(state);
                r.hash(state);
            }
            StorageKey::Manifest(m) => {
                m.registry().hash(state);
                m.repository().hash(state);
                m.version().hash(state);
            }
        }
    }
}

fn push_temp_path<'a>(buf: &'a mut PathBuf) -> Result<&'a mut PathBuf, ImageError> {
    let pid = std::process::id();
    let ts = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(_) => 0,
    };
    push_checked_path(buf, "tmp")?;
    push_checked_path(buf, &format!("{}.{}", pid, ts))?;
    Ok(buf)
}

fn push_checked_path<'a>(buf: &'a mut PathBuf, path: &str) -> Result<&'a mut PathBuf, ImageError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^[a-zA-Z0-9]+[a-zA-Z0-9_\.\-]*$").unwrap();
    }
    if RE.is_match(path) {
        buf.push(path);
        Ok(buf)
    } else {
        Err(ImageError::BadStoragePath(path.to_string()))
    }
}

impl StorageKey {
    fn to_path(&self, base_dir: &Path) -> Result<PathBuf, ImageError> {
        match self {
            StorageKey::Blob(digest) => {
                let mut path = base_dir.to_path_buf();
                push_checked_path(&mut path, "blobs")?;
                push_checked_path(&mut path, &digest.replace(":", "_"))?;
                Ok(path)
            }
            StorageKey::BlobPart(digest, range) => {
                let mut path = base_dir.to_path_buf();
                push_checked_path(&mut path, "parts")?;
                push_checked_path(&mut path, &digest.replace(":", "_"))?;
                push_checked_path(&mut path, &format!("{:x}_{:x}", range.start, range.end))?;
                Ok(path)
            }
            StorageKey::Manifest(reference) => {
                let mut path = base_dir.to_path_buf();
                push_checked_path(&mut path, "manifest")?;
                push_checked_path(&mut path, &reference.registry())?;
                push_checked_path(&mut path, &reference.repository().replace("/", "_"))?;
                push_checked_path(
                    &mut path,
                    &reference.version().replace("@", "").replace(":", ""),
                )?;
                Ok(path)
            }
        }
    }
}
