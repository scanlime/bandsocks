use crate::{
    errors::ImageError,
    image::{ContentDigest, ImageVersion, Registry, Repository},
};
use memmap::{Mmap, MmapOptions};
use pin_project::pin_project;
use sha2::{Digest, Sha256};
use std::{
    hash::Hash,
    ops::Range,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
    time::SystemTime,
};
use tokio::{
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

    pub async fn mmap(&self, key: &StorageKey) -> Result<Option<Mmap>, ImageError> {
        match self.open(key).await? {
            Some(file) => {
                let file = file.into_std().await;
                Ok(Some(unsafe { MmapOptions::new().map(&file) }?))
            }
            None => Ok(None),
        }
    }

    pub async fn begin_write(&self) -> Result<StorageWriter, ImageError> {
        let mut temp_path = self.path.clone();
        let pid = std::process::id();
        let ts = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_millis(),
            Err(_) => 0,
        };
        temp_path.push("tmp");
        temp_path.push(format!("{}.{}", pid, ts));
        create_parent_dirs(&temp_path).await;

        let temp_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o440)
            .open(&temp_path)
            .await?;

        Ok(StorageWriter {
            temp_file: Some(temp_file),
            temp_path: Some(temp_path),
            hasher: Some(Sha256::new()),
            content_digest: None,
        })
    }

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
    pub async fn remove_temp(&mut self) -> Result<(), ImageError> {
        if let Some(path) = self.temp_path.take() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    pub async fn rename_temp(&mut self, dest_path: &Path) -> Result<(), ImageError> {
        let temp_path = self
            .temp_path
            .take()
            .expect("storage writer temp can only be taken once");
        tokio::fs::rename(&temp_path, &dest_path).await?;
        Ok(())
    }

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

#[derive(Debug)]
pub struct TempStorage {
    path: Option<PathBuf>,
    pub content_digest: Result<ContentDigest, ImageError>,
}

impl Drop for TempStorage {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            log::warn!("leaking temporary storage at {:?}", path);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum StorageKey {
    Blob(ContentDigest),
    BlobPart(ContentDigest, Range<usize>),
    Manifest(Registry, Repository, ImageVersion),
}

impl StorageKey {
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
}

impl StorageKey {
    fn to_path(&self, base_dir: &Path) -> PathBuf {
        match self {
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
