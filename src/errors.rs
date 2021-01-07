//! Error types you might see while setting up or running a container

use crate::sand::protocol::Errno;
use thiserror::Error;

/// Errors during container image preparation
#[derive(Error, Debug)]
pub enum ImageError {
    /// invalid image reference format
    #[error("invalid image reference format: {0:?}")]
    InvalidReferenceFormat(String),

    /// storage io error
    #[error("storage io error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("utf8 path conversion error")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// json error
    #[error("json error: {0}")]
    JSON(#[from] serde_json::Error),

    /// asynchronous task failed during image preparation
    #[error("asynchronous task failed during image preparation")]
    TaskJoin(#[from] tokio::task::JoinError),

    /// pull task terminated unexpectedly
    #[error("pull task terminated unexpectedly")]
    PullTaskError,

    /// asynchronous image hashing task failed during image preparation
    #[error("asynchronous image hashing task failed during image preparation")]
    ByteChannelError(#[from] std::sync::mpsc::SendError<bytes::Bytes>),

    /// network request error
    #[error("network request error: {0}")]
    NetworkRequest(#[from] reqwest::Error),

    /// string in image configuration contained internal nul byte
    #[error("string in image configuration contained internal nul byte")]
    NulStringError(#[from] std::ffi::NulError),

    /// registry server is not allowed by the current configuration
    #[error("registry server is not allowed by the current configuration: {0}")]
    RegistryNotAllowed(crate::image::Registry),

    /// tar file format error
    #[error("tar file format error")]
    TARFileError,

    /// virtual filesystem error while preparing image
    #[error("virtual filesystem error while preparing image: {0}")]
    ImageVFSError(#[from] VFSError),

    /// data just written to the cache is missing
    #[error("data just written to the cache is missing")]
    StorageMissingAfterInsert,

    /// i/o errors occurred, the content digest is not valid
    #[error("i/o errors occurred, the content digest is not valid")]
    ContentDigestIOError,

    /// we are in offline mode, but a download was requested
    #[error("we are in offline mode, but a download was requested")]
    DownloadInOfflineMode,

    /// can't determine where to cache image files
    #[error("can't determine where to cache image files")]
    NoDefaultCacheDir,

    /// only v2 image manifests are supported
    #[error("only v2 image manifests are supported")]
    UnsupportedManifestType,

    /// unsupported type for runtime config
    #[error("unsupported type for runtime config, {0:?}")]
    UnsupportedRuntimeConfigType(String),

    /// unsupported type for image layer
    #[error("unsupported type for image layer, {0:?}")]
    UnsupportedLayerType(String),

    /// invalid content type string
    #[error("invalid content type string, {0:?}")]
    InvalidContentType(String),

    /// unexpected content size
    #[error("unexpected content size")]
    UnexpectedContentSize,

    /// unable to locate decompressed layers by content hash
    #[error("unable to locate decompressed layers by content hash")]
    UnexpectedDecompressedLayerContent,

    /// unsupported type for rootfs in image config
    #[error("unsupported type for rootfs in image config, {0:?}")]
    UnsupportedRootFilesystemType(String),

    /// insecure configuration; refusing to run a manifest downloaded over HTTP
    /// with no content digest
    #[error("insecure configuration; refusing to run a manifest downloaded over HTTP with no content digest")]
    InsecureManifest,

    /// registry server requested an unsupported type of authentication
    #[error("registry server requested an unsupported type of authentication: {0:?}")]
    UnsupportedAuthentication(String),

    /// calculated digest of downloaded content is not what we asked for
    #[error("calculated digest of downloaded content is not what we asked for, expected {expected}, found {found}")]
    ContentDigestMismatch {
        expected: crate::image::ContentDigest,
        found: crate::image::ContentDigest,
    },
}

/// Errors that occur while a container is running
#[derive(Error, Debug)]
pub enum RuntimeError {
    /// io error
    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),

    /// protocol error
    #[error("protocol error: {0}")]
    ProtocolError(#[from] crate::sand::protocol::buffer::Error),

    /// connection lost unexpectedly
    #[error("connection lost unexpectedly")]
    Disconnected,

    /// file queue full error
    #[error("file queue full error")]
    FileQueueFullError(#[from] fd_queue::QueueFullError),

    /// container image error
    #[error("container image error: {0}")]
    ImageError(#[from] ImageError),

    /// task join error
    #[error("task join error: {0}")]
    TaskJoinError(#[from] tokio::task::JoinError),

    /// virtual filesystem error
    #[error("virtual filesystem error: {0}")]
    VFSError(#[from] VFSError),

    /// container has no configured entry point
    #[error("container has no configured entry point")]
    NoEntryPoint,

    /// invalid process ID
    #[error("invalid process ID")]
    InvalidPid,

    /// incorrect ipc process state
    #[error("incorrect ipc process state")]
    WrongProcessState,

    /// string decoding error
    #[error("string decoding error")]
    StringDecoding,

    /// failed to allocate sandbox process
    #[error("failed to allocate sandbox process: {0}")]
    ProgramAllocError(String),

    /// memory access error
    #[error("memory access error")]
    MemAccess,

    /// error in memory-backed file
    #[error("error in memory-backed file: {0}")]
    MemfdError(#[from] memfd::Error),

    /// argument string contained internal nul byte
    #[error("argument string contained internal nul byte")]
    NulStringError(#[from] std::ffi::NulError),

    /// unexpected exit status from sandbox runtime
    #[error("unexpected exit from sandbox runtime, {status}\n{stderr}")]
    SandUnexpectedStatus {
        status: std::process::ExitStatus,
        stderr: String,
    },

    /// sandbox runtime reported an unexpected disconnect
    #[error("sandbox runtime reports an unexpected disconnect\n{stderr}")]
    SandReportsDisconnect { stderr: String },

    /// sandbox runtime reports a low-level I/O error
    #[error("sandbox runtime reports a low-level I/O error\n{stderr}")]
    SandIOError { stderr: String },

    /// panic from sandbox runtime
    #[error("panic from sandbox runtime\n{stderr}")]
    SandPanic { stderr: String },

    /// out of memory in sandbox runtime
    #[error("out of memory in sandbox runtime\n{stderr}")]
    SandOutOfMem { stderr: String },
}

/// Errors from the virtual filesystem layer, convertible to an errno code
#[derive(Error, Clone, Debug)]
pub enum VFSError {
    #[error("unexpected filesystem image storage error")]
    ImageStorageError,

    #[error("generic I/O error")]
    IO,

    #[error("expected a directory, found another node type")]
    DirectoryExpected,

    #[error("expected a file, found another node type")]
    FileExpected,

    #[error("expected a symlink, found another node type")]
    LinkExpected,

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

    #[error("utf8 path conversion error")]
    Utf8Error(#[from] std::str::Utf8Error),
}

impl VFSError {
    /// Convert this error to the equivalent kernel errno value
    pub fn to_errno(&self) -> libc::c_int {
        match self {
            VFSError::ImageStorageError => libc::EIO,
            VFSError::Utf8Error(_) => libc::EINVAL,
            VFSError::IO => libc::EIO,
            VFSError::DirectoryExpected => libc::ENOTDIR,
            VFSError::FileExpected => libc::EISDIR,
            VFSError::LinkExpected => libc::EINVAL,
            VFSError::UnallocNode => libc::ENOENT,
            VFSError::NotFound => libc::ENOENT,
            VFSError::PathSegmentLimitExceeded => libc::ENAMETOOLONG,
            VFSError::SymbolicLinkLimitExceeded => libc::ELOOP,
            VFSError::INodeRefCountError => libc::ENOMEM,
        }
    }
}

impl From<VFSError> for Errno {
    fn from(err: VFSError) -> Self {
        Errno(-err.to_errno())
    }
}
