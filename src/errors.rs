use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("registry error: {0}")]
    Registry(#[from] dkregistry::errors::Error),

    #[error("storage io error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("json error: {0}")]
    JSON(#[from] serde_json::Error),

    #[error("tar file format error")]
    TARFileError,

    #[error("virtual filesystem error while preparing image: {0}")]
    ImageVFSError(#[from] VFSError),

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

#[derive(Error, Debug)]
pub enum VFSError {
    #[error("expected a directory, found another node type")]
    DirectoryExpected,

    #[error("expected a file, found another node type")]
    FileExpected,

    #[error("unallocated node")]
    UnallocNode,

    #[error("not found")]
    NotFound,

    #[error("too many nested path segments")]
    PathSegmentLimitExceeded,

    #[error("too many nested symbolic links")]
    SymbolicLinkLimitExceeded,

    #[error("inode reference count error")]
    INodeRefCountError,
}

impl VFSError {
    pub fn to_errno(&self) -> libc::c_int {
        match self {
            VFSError::DirectoryExpected => libc::ENOTDIR,
            VFSError::FileExpected => libc::EISDIR,
            VFSError::UnallocNode => libc::ENOENT,
            VFSError::NotFound => libc::ENOENT,
            VFSError::PathSegmentLimitExceeded => libc::ENAMETOOLONG,
            VFSError::SymbolicLinkLimitExceeded => libc::ELOOP,
            VFSError::INodeRefCountError => libc::ENOMEM,
        }
    }
}

#[derive(Error, Debug)]
pub enum IPCError {
    #[error("ipc io error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("ipc protocol error: {0}")]
    ProtocolError(#[from] crate::sand::protocol::buffer::Error),

    #[error("file queue full error")]
    FileQueueFullError(#[from] fd_queue::QueueFullError),

    #[error("invalid process ID")]
    InvalidPid,

    #[error("incorrect ipc process state")]
    WrongProcessState,

    #[error("string decoding error")]
    StringDecoding,

    #[error("memory access error")]
    MemAccess,
}

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("runtime io error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("virtual filesystem error: {0}")]
    VFSError(#[from] VFSError),

    #[error("interprocess communication error: {0}")]
    IPCError(#[from] IPCError),

    #[error("task join error: {0}")]
    TaskJoinError(#[from] tokio::task::JoinError),

    #[error("container image error: {0}")]
    ImageError(#[from] ImageError),

    #[error("container has no configured image")]
    NoImage,

    #[error("container has no configured entry point")]
    NoEntryPoint,
}
