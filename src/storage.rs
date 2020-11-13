use crate::{
    errors::ImageError,
    filesystem::mmap::{MapRef, WeakMapRef},
    Reference,
};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::io::AsyncWriteExt;

pub struct FileStorage {
    path: PathBuf,
    memo: HashMap<StorageKey, WeakMapRef>,
}

impl FileStorage {
    pub fn new(path: PathBuf) -> Self {
        FileStorage {
            path,
            memo: HashMap::new(),
        }
    }

    pub fn get(&mut self, key: &StorageKey) -> Result<Option<MapRef>, ImageError> {
        log::debug!("storage get, {:?}", key);
        match self.memo.get(key).and_then(|weak| weak.upgrade()) {
            Some(arc) => {
                log::debug!("storage get, {:?}, succeeded from memo", key);
                Ok(Some(arc))
            }
            None => {
                let path = key.to_path(&self.path)?;
                match MapRef::open(&path) {
                    Err(e) => match e.kind() {
                        std::io::ErrorKind::NotFound => Ok(None),
                        _ => Err(e.into()),
                    },
                    Ok(mapref) => {
                        log::debug!("storage get, {:?}, succeeded opening new mapping", key);
                        self.memo.insert(key.clone(), mapref.downgrade());
                        Ok(Some(mapref))
                    }
                }
            }
        }
    }

    pub async fn insert(&mut self, key: &StorageKey, data: Vec<u8>) -> Result<MapRef, ImageError> {
        log::debug!("Storage insert, {:?}, {} bytes", key, data.len());

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
        let mut file = tokio::fs::File::create(&temp_path).await?;
        file.write_all(&data).await?;
        file.flush().await?;
        std::mem::drop(file);

        // This part is atomic
        tokio::fs::rename(temp_path, dest_path).await?;

        // The resulting mmap might be a different file than the one we just wrote, if
        // another process or thread was racing with us. The content should be
        // identical.
        match self.get(key)? {
            Some(mapping) => Ok(mapping),
            None => Err(ImageError::StorageMissingAfterInsert),
        }
    }
}

#[derive(Clone, Debug)]
pub enum StorageKey {
    Blob(String),
    Manifest(Reference),
}

impl StorageKey {
    pub fn from_blob_data(data: &[u8]) -> StorageKey {
        StorageKey::Blob(format!("sha256:{:x}", Sha256::digest(data)))
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
