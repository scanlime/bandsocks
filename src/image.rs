use crate::{
    filesystem::{storage::FileStorage, vfs::Filesystem},
    manifest::RuntimeConfig,
};

pub struct Image {
    pub digest: String,
    pub config: RuntimeConfig,
    pub filesystem: Filesystem,
    pub storage: FileStorage,
}
