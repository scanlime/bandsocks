use serde::{Deserialize, Serialize};

/// Partial implementation of the manifest v2 schema2 spec.
/// The dkregistry crate has its own partial version of this spec too, but it's mostly private.
/// https://docs.docker.com/registry/spec/manifest-v2-2/
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Manifest {
    pub config: Link,
    pub layers: Vec<Link>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Link {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    pub digest: String,
}

pub mod media_types {
    pub const RUNTIME_CONFIG: &'static str = "application/vnd.docker.container.image.v1+json";
    pub const LAYER_TAR_GZIP: &'static str = "application/vnd.docker.image.rootfs.diff.tar.gzip";
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub architecture: String,
    pub config: ImageConfig,
    pub created: String,
    pub docker_version: String,
    pub os: String,
    pub rootfs: Filesystem,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ImageConfig {
    #[serde(rename = "User")]
    pub user: String,
    #[serde(rename = "Env")]
    pub env: Vec<String>,
    #[serde(rename = "Cmd")]
    pub cmd: Vec<String>,
    #[serde(rename = "Image")]
    pub image: String,
    #[serde(rename = "WorkingDir")]
    pub working_dir: String,
    #[serde(rename = "Entrypoint")]
    pub entrypoint: Option<Vec<String>>,
}

pub const FS_TYPE: &'static str = "layers";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Filesystem {
    #[serde(rename = "type")]
    pub fs_type: String,
    pub diff_ids: Vec<String>,
}
