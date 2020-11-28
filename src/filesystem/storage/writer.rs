use crate::{errors::ImageError, filesystem::storage::StorageKey, image::ContentDigest};
use pin_project::pin_project;
use sha2::{Digest, Sha256};
use std::{
    fs,
    fs::File,
    io,
    io::Write,
    path::{Path, PathBuf},
};

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

impl StorageWriter {
    pub fn new(key: StorageKey, temp_file: File, temp_path: PathBuf) -> StorageWriter {
        StorageWriter {
            key,
            temp_file: Some(temp_file),
            temp_path: Some(temp_path),
            hasher: Some(Sha256::new()),
            content_digest: None,
        }
    }

    /// Delete the temporary file backing this writer
    pub fn remove_temp(&mut self) -> Result<(), ImageError> {
        if let Some(path) = self.temp_path.take() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Rename the temporary file and detach it from this writer
    pub fn rename_temp(&mut self, dest_path: &Path) -> Result<(), ImageError> {
        let temp_path = self
            .temp_path
            .take()
            .expect("storage writer temp can only be taken once");
        fs::rename(&temp_path, &dest_path)?;
        Ok(())
    }

    /// Flush buffered I/O and return the final content digest
    pub fn finalize(&mut self) -> Result<ContentDigest, ImageError> {
        self.flush()?;
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

impl Write for StorageWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        let result = self
            .temp_file
            .as_ref()
            .expect("storage writer open")
            .write(buf);
        match result {
            Err(e) => {
                self.content_digest = Some(Err(()));
                Err(e)
            }
            Ok(actual_size) => {
                if let Some(hasher) = &mut self.hasher {
                    hasher.update(&buf[..actual_size]);
                }
                Ok(actual_size)
            }
        }
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        let result = self
            .temp_file
            .as_ref()
            .expect("storage writer open")
            .flush();
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                self.content_digest = Some(Err(()));
                Err(e)
            }
        }
    }
}
