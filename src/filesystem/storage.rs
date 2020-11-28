use crate::{
    errors::ImageError,
    image::{ContentDigest, ImageVersion, Registry, Repository},
};
use memmap::{Mmap, MmapOptions};
use pin_project::pin_project;
use sha2::{Digest, Sha256};
use std::{
    env,
    hash::Hash,
    ops::Range,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    fs,
    fs::{os::unix::OpenOptionsExt, File, OpenOptions},
    io,
    io::{AsyncWrite, AsyncWriteExt},
};

#[derive(Clone, Debug)]
pub struct FileStorage {
    path: PathBuf,
}

impl FileStorage {
    pub fn new(path: PathBuf) -> Self {
        FileStorage { path }
    }

    /// Open one object from local storage, as a File
    pub async fn open(&self, key: &StorageKey) -> Result<Option<File>, ImageError> {
        let path = key.to_path(&self.path);
        match File::open(path).await {
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Ok(None),
                _ => Err(e.into()),
            },
            Ok(f) => Ok(Some(f)),
        }
    }

    /// Open an object, creating requested BlobParts on demand
    pub async fn open_part(&self, key: &StorageKey) -> Result<Option<File>, ImageError> {
        match self.open(key).await? {
            Some(f) => Ok(Some(f)),
            None => match key.clone() {
                StorageKey::BlobPart(digest, range) => {
                    match self.mmap(&StorageKey::Blob(digest)).await? {
                        None => Ok(None),
                        Some(part_of) => {
                            let mut part = &part_of[range];
                            let mut writer = self.begin_write().await?;
                            io::copy(&mut part, &mut writer).await?;
                            self.commit_write(writer, key).await?;
                            self.open(key).await
                        }
                    }
                }
                _ => Ok(None),
            },
        }
    }

    /// Check whether a stored file exists without actually opening it
    ///
    /// Returns true if and only if the storage exists as a regular file. Any
    /// errors will cause this to return false.
    pub async fn exists(&self, key: &StorageKey) -> bool {
        let path = key.to_path(&self.path);
        match fs::metadata(path).await {
            Err(_) => false,
            Ok(metadata) => metadata.is_file(),
        }
    }

    /// Check whether a set of stored files all exist
    pub async fn all_exists<'a, T: Iterator<Item = &'a StorageKey>>(&self, keys: T) -> bool {
        for item in keys {
            if !self.exists(item).await {
                return false;
            }
        }
        true
    }

    /// Make a new storage object at `to_key` using the data from `from_key`
    pub async fn copy_data(
        &self,
        from_key: &StorageKey,
        to_key: &StorageKey,
    ) -> Result<bool, ImageError> {
        let source = self.mmap(from_key).await?;
        if let Some(source) = source {
            let mut slice = &source[..];
            let mut writer = self.begin_write().await?;
            io::copy(&mut slice, &mut writer).await?;
            self.commit_write(writer, to_key).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Open a storage object and memory map it
    pub async fn mmap(&self, key: &StorageKey) -> Result<Option<Mmap>, ImageError> {
        match self.open(key).await? {
            Some(file) => {
                let file = file.into_std().await;
                Ok(Some(unsafe { MmapOptions::new().map(&file) }?))
            }
            None => Ok(None),
        }
    }

    /// Begin writing to temporary storage
    pub async fn begin_write(&self) -> Result<StorageWriter, ImageError> {
        let key = StorageKey::temp();
        let temp_path = key.to_path(&self.path);
        create_parent_dirs(&temp_path).await;

        let temp_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o440)
            .open(&temp_path)
            .await?;

        Ok(StorageWriter {
            key,
            temp_file: Some(temp_file),
            temp_path: Some(temp_path),
            hasher: Some(Sha256::new()),
            content_digest: None,
        })
    }

    /// Promote a temporary file into a StorageKey
    pub async fn commit_write(
        &self,
        mut writer: StorageWriter,
        key: &StorageKey,
    ) -> Result<(), ImageError> {
        let content_digest = writer.finalize().await?;
        let dest_path = key.to_path(&self.path);
        create_parent_dirs(&dest_path).await;
        writer.rename_temp(&dest_path).await?;
        log::debug!("storage commit, {:?} -> {:?}", content_digest, dest_path);
        Ok(())
    }
}

async fn create_parent_dirs(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Err(err) = tokio::fs::create_dir_all(parent).await {
            // Log a warning instead of giving up right away, in case this was a race
            // condition.
            log::warn!("error creating temp directory at {:?}, {:?}", parent, err);
        }
    }
}

#[pin_project]
#[derive(Debug)]
pub struct StorageWriter {
    #[pin]
    temp_file: Option<File>,
    hasher: Option<Sha256>,
    temp_path: Option<PathBuf>,
    content_digest: Option<Result<ContentDigest, ()>>,
    pub key: StorageKey,
}

impl AsyncWrite for StorageWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let poll = self
            .as_mut()
            .project()
            .temp_file
            .as_pin_mut()
            .expect("storage writer open")
            .poll_write(cx, buf);
        match poll {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => {
                self.as_mut().content_digest = Some(Err(()));
                Poll::Ready(Err(e))
            }
            Poll::Ready(Ok(actual_size)) => {
                if let Some(hasher) = &mut self.hasher {
                    hasher.update(&buf[..actual_size]);
                }
                Poll::Ready(Ok(actual_size))
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.as_mut()
            .project()
            .temp_file
            .as_pin_mut()
            .expect("storage writer open")
            .poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        let poll = self
            .as_mut()
            .project()
            .temp_file
            .as_pin_mut()
            .expect("storage writer open")
            .poll_shutdown(cx);
        match poll {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => {
                self.content_digest = Some(Err(()));
                Poll::Ready(Err(e))
            }
        }
    }
}

impl StorageWriter {
    /// Delete the temporary file backing this writer
    pub async fn remove_temp(&mut self) -> Result<(), ImageError> {
        if let Some(path) = self.temp_path.take() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    /// Rename the temporary file and detach it from this writer
    pub async fn rename_temp(&mut self, dest_path: &Path) -> Result<(), ImageError> {
        let temp_path = self
            .temp_path
            .take()
            .expect("storage writer temp can only be taken once");
        tokio::fs::rename(&temp_path, &dest_path).await?;
        Ok(())
    }

    /// Wait for I/O to complete, and return the final ContentDigest of the
    /// written data
    pub async fn finalize(&mut self) -> Result<ContentDigest, ImageError> {
        if let Some(mut temp_file) = self.temp_file.take() {
            temp_file.shutdown().await?;
        }
        if let Some(hasher) = self.hasher.take() {
            if self.content_digest.is_none() {
                self.content_digest =
                    Some(Ok(ContentDigest::from_parts("sha256", &hasher.finalize())
                        .expect("always parseable")));
            }
        }
        self.content_digest
            .as_ref()
            .expect("storage writer should be ready to finalize")
            .clone()
            .map_err(|()| ImageError::ContentDigestIOError)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum StorageKey {
    Temp(u32, u64),
    Blob(ContentDigest),
    BlobPart(ContentDigest, Range<usize>),
    Manifest(Registry, Repository, ImageVersion),
}

impl StorageKey {
    pub fn temp() -> Self {
        StorageKey::Temp(std::process::id(), rand::random::<u64>())
    }

    pub fn range(self, sub_range: Range<usize>) -> Result<StorageKey, ()> {
        match self {
            StorageKey::Blob(content_digest) => Ok(StorageKey::BlobPart(content_digest, sub_range)),
            StorageKey::BlobPart(parent_digest, parent_range) => Ok(StorageKey::BlobPart(
                parent_digest,
                (sub_range.start + parent_range.start)..(sub_range.end + parent_range.start),
            )),
            _ => Err(()),
        }
    }

    fn to_path(&self, base_dir: &Path) -> PathBuf {
        match self {
            StorageKey::Temp(pid, random) => {
                let mut path = base_dir.to_path_buf();
                path.push("tmp");
                path.push(format!("{}-{}", pid, random));
                path
            }
            StorageKey::Blob(content_digest) => {
                let mut path = base_dir.to_path_buf();
                path.push("blobs");
                path.push(content_digest.as_str());
                path
            }
            StorageKey::BlobPart(content_digest, range) => {
                let mut path = base_dir.to_path_buf();
                path.push("parts");
                path.push(content_digest.as_str());
                path.push(&format!("{:x}:{:x}", range.start, range.end));
                path
            }
            StorageKey::Manifest(registry, repository, version) => {
                let mut path = base_dir.to_path_buf();
                path.push("manifest");
                path.push(registry.as_str());
                path.push(repository.as_str().replace('/', ":"));
                path.push(version.as_str());
                path
            }
        }
    }
}

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
