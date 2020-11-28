use crate::image::{ContentDigest, ImageVersion, Registry, Repository};
use std::{
    hash::Hash,
    ops::Range,
    path::{Path, PathBuf},
};

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

    pub fn to_path(&self, base_dir: &Path) -> PathBuf {
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
