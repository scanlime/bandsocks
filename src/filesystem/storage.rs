use crate::{
    errors::ImageError,
    image::{ContentDigest, ImageName},
};
use memmap::{Mmap, MmapOptions};
use std::{
    hash::Hash,
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
        log::debug!("insert {:?}, {} bytes", key, data.len());

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

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum StorageKey {
    Blob(ContentDigest),
    Manifest(ImageName),
    BlobPart(ContentDigest, Range<usize>),
}

impl StorageKey {
    pub fn from_blob_data(data: &[u8]) -> StorageKey {
        StorageKey::Blob(ContentDigest::from_content(data))
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

fn push_temp_path<'a>(buf: &'a mut PathBuf) -> Result<&'a mut PathBuf, ImageError> {
    let pid = std::process::id();
    let ts = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(_) => 0,
    };
    buf.push("tmp");
    buf.push(format!("{}.{}", pid, ts));
    Ok(buf)
}

impl StorageKey {
    fn to_path(&self, base_dir: &Path) -> Result<PathBuf, ImageError> {
        match self {
            StorageKey::Blob(digest) => {
                let mut path = base_dir.to_path_buf();
                path.push("blobs");
                path.push(digest.as_str());
                Ok(path)
            }
            StorageKey::BlobPart(digest, range) => {
                let mut path = base_dir.to_path_buf();
                path.push("parts");
                path.push(digest.as_str());
                path.push(&format!("{:x}_{:x}", range.start, range.end));
                Ok(path)
            }
            StorageKey::Manifest(image_name) => {
                let mut path = base_dir.to_path_buf();
                path.push("manifest");
                if let Some(registry) = image_name.registry_str() {
                    path.push(registry);
                }
                path.push(&image_name.repository_str());
                if let Some(tag) = image_name.tag_str() {
                    path.push(tag);
                }
                if let Some(digest) = image_name.content_digest_str() {
                    path.push(digest);
                }
                Ok(path)
            }
        }
    }
}
