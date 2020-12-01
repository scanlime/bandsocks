//! Error types you might see while setting up or running a container

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("invalid image reference format: {0:?}")]
    InvalidReferenceFormat(String),

    #[error("storage io error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("json error: {0}")]
    JSON(#[from] serde_json::Error),

    #[error("asynchronous task failed during image preparation")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("asynchronous image hashing task failed during image preparation")]
    ByteChannelError(#[from] std::sync::mpsc::SendError<bytes::Bytes>),

    #[error("network request error: {0}")]
    NetworkRequest(#[from] reqwest::Error),

    #[error("string in image configuratoin contained internal nul byte")]
    NulStringError(#[from] std::ffi::NulError),

    #[error("registry server is not allowed by the current configuration: {0}")]
    RegistryNotAllowed(crate::image::Registry),

    #[error("tar file format error")]
    TARFileError,

    #[error("virtual filesystem error while preparing image: {0}")]
    ImageVFSError(#[from] VFSError),

    #[error("data just written to the cache is missing")]
    StorageMissingAfterInsert,

    #[error("i/o errors occurred, the content digest is not valid")]
    ContentDigestIOError,

    #[error("we are in offline mode, but a download was requested")]
    DownloadInOfflineMode,

    #[error("calculated digest of downloaded content is not what we asked for, expected {expected}, found {found}")]
    ContentDigestMismatch {
        expected: crate::image::ContentDigest,
        found: crate::image::ContentDigest,
    },

    #[error("can't determine where to cache image files")]
    NoDefaultCacheDir,

    #[error("only v2 image manifests are supported")]
    UnsupportedManifestType,

    #[error("unsupported type for runtime config, {0:?}")]
    UnsupportedRuntimeConfigType(String),

    #[error("unsupported type for image layer, {0:?}")]
    UnsupportedLayerType(String),

    #[error("invalid content type string, {0:?}")]
    InvalidContentType(String),

    #[error("unexpected content size")]
    UnexpectedContentSize,

    #[error("unable to locate decompressed layers by content hash")]
    UnexpectedDecompressedLayerContent,

    #[error("unsupported type for rootfs in image config, {0:?}")]
    UnsupportedRootFilesystemType(String),

    #[error("insecure configuration; refusing to run a manifest downloaded over HTTP with no content digest")]
    InsecureManifest,

    #[error("registry server requested an unsupported type of authentication: {0:?}")]
    UnsupportedAuthentication(String),
}

#[derive(Error, Debug)]
pub enum VFSError {
    #[error("unexpected filesystem image storage error")]
    ImageStorageError,

    #[error("generic I/O error")]
    IO,

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
            VFSError::ImageStorageError => libc::EIO,
            VFSError::IO => libc::EIO,
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

    #[error("task join error: {0}")]
    TaskJoinError(#[from] tokio::task::JoinError),

    #[error("invalid process ID")]
    InvalidPid,

    #[error("incorrect ipc process state")]
    WrongProcessState,

    #[error("string decoding error")]
    StringDecoding,

    #[error("failed to allocate sandbox process: {0}")]
    ProgramAllocError(String),

    #[error("memory access error")]
    MemAccess,

    #[error("error in memory-backed file: {0}")]
    MemfdError(#[from] memfd::Error),

    #[error("connection lost unexpectedly")]
    Disconnected,

    #[error("exception from sandbox runtime, {status}\n{stderr}")]
    SandError {
        status: std::process::ExitStatus,
        stderr: String,
    },
}

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("runtime io error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("virtual filesystem error: {0}")]
    VFSError(#[from] VFSError),

    #[error("interprocess communication error: {0}")]
    IPCError(#[from] IPCError),

    #[error("argument string contained internal nul byte")]
    NulStringError(#[from] std::ffi::NulError),

    #[error("task join error: {0}")]
    TaskJoinError(#[from] tokio::task::JoinError),

    #[error("container image error: {0}")]
    ImageError(#[from] ImageError),

    #[error("container has no configured entry point")]
    NoEntryPoint,
}
