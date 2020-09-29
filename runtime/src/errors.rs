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
}
