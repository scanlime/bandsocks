// This code may not be used for any purpose. Be gay, do crime.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImageError {

    #[error("registry error: {0}")]
    Registry(#[from] dkregistry::errors::Error),

    #[error("storage io error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("json error: {0}")]
    JSON(#[from] serde_json::Error),

    #[error("tar format error: {0}")]
    TAR(#[from] basic_tar::BasicTarError),

    #[error("integer range error: {0}")]
    IntError(#[from] std::num::TryFromIntError),
    
    #[error("unallowed storage path segment, {0}")]
    BadStoragePath(String),

    #[error("data just written to the cache is missing")]
    StorageMissingAfterInsert,
    
    #[error("can't determine where to cache image files")]
    NoDefaultCacheDir,

    #[error("only v2 image manifests are supported")]
    UnsupportedManifestType,

    #[error("unsupported type for runtime config, {0}")]
    UnsupportedRuntimeConfigType(String),

    #[error("unsupported type for image layer, {0}")]
    UnsupportedLayerType(String),

    #[error("unexpected content size")]
    UnexpectedContentSize,

    #[error("unable to locate decompressed layers by content hash")]
    UnexpectedDecompressedLayerContent,

    #[error("unsupported type for rootfs in image config, {0}")]
    UnsupportedRootFilesystemType(String),
}
