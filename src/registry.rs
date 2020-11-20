//! Optional lower-level interface to the container registry and cache

use crate::{
    errors::ImageError,
    filesystem::{
        storage::{FileStorage, StorageKey},
        tar, vfs,
    },
    image::Image,
    manifest::{media_types, Link, Manifest, RuntimeConfig, FS_TYPE},
    Reference,
};

use directories_next::ProjectDirs;
use dkregistry::v2::Client as RegistryClient;
use flate2::read::GzDecoder;
use futures_util::{stream::FuturesUnordered, StreamExt};
use memmap::Mmap;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::task;

/// Builder for configuring custom [Client] instances
pub struct ClientBuilder {
    cache_dir: Option<PathBuf>,
}

impl ClientBuilder {
    /// Change the cache directory used for read-only copies of downloaded
    /// images.
    pub fn cache_dir(&mut self, dir: &Path) -> &mut Self {
        self.cache_dir = Some(dir.to_path_buf());
        self
    }

    /// Determine a default per-user cache directory which will be used if an
    /// alternate cache directory is not specified. This will typically
    /// return `$HOME/.cache/bandsocks`
    pub fn default_cache_dir() -> Result<PathBuf, ImageError> {
        if let Some(proj_dirs) = ProjectDirs::from("org", "scanlime", "bandsocks") {
            Ok(proj_dirs.cache_dir().to_path_buf())
        } else {
            Err(ImageError::NoDefaultCacheDir)
        }
    }

    /// Construct a Client using the parameters from this Builder
    pub fn build(self) -> Result<Client, ImageError> {
        let cache_dir = match self.cache_dir {
            Some(dir) => dir,
            None => ClientBuilder::default_cache_dir()?,
        };
        Ok(Client {
            storage: FileStorage::new(cache_dir),
        })
    }
}

/// Registry clients can download and store data from an image registry
#[derive(Clone)]
pub struct Client {
    storage: FileStorage,
}

impl Client {
    /// Construct a new registry client with default options
    pub fn new() -> Result<Client, ImageError> {
        Client::configure().build()
    }

    /// Construct a registry client with custom options, via ClientBuilder
    pub fn configure() -> ClientBuilder {
        ClientBuilder { cache_dir: None }
    }

    async fn registry_client_for(
        &mut self,
        image: &Reference,
    ) -> Result<RegistryClient, ImageError> {
        let client = RegistryClient::configure()
            .registry(&image.registry())
            .insecure_registry(false)
            .username(None)
            .password(None)
            .build()?;
        let login_scope = format!("repository:{}:pull", image.repository());
        Ok(client.authenticate(&[&login_scope]).await?)
    }

    async fn pull_manifest(&mut self, image: &Reference) -> Result<Manifest, ImageError> {
        let key = StorageKey::Manifest(image.clone());
        let map = match self.storage.mmap(&key).await? {
            Some(map) => map,
            None => {
                log::info!("{} downloading manifest...", image);
                let rc = self.registry_client_for(image).await?;
                match rc
                    .get_manifest(&image.repository(), &image.version())
                    .await?
                {
                    dkregistry::v2::manifest::Manifest::S2(schema) => {
                        // FIXME: need to verify sha256 of the manifest.
                        // multiple problems with using dkregistry here at this point. time to
                        // switch tactics?
                        let spec_data = serde_json::to_vec(&schema.manifest_spec)?;
                        self.storage.insert(&key, &spec_data).await?;
                        match self.storage.mmap(&key).await? {
                            Some(map) => map,
                            None => return Err(ImageError::StorageMissingAfterInsert),
                        }
                    }
                    _ => return Err(ImageError::UnsupportedManifestType),
                }
            }
        };
        let slice = &map[..];
        log::trace!("raw json manifest, {}", String::from_utf8_lossy(slice));
        Ok(serde_json::from_slice(slice)?)
    }

    async fn local_blob(&mut self, digest: &str) -> Result<Option<StorageKey>, ImageError> {
        let key = StorageKey::Blob(digest.to_string());
        match self.storage.open(&key).await {
            Err(e) => Err(e),
            Ok(None) => Ok(None),
            Ok(_map) => Ok(Some(key)),
        }
    }

    async fn local_blob_list(
        &mut self,
        digest_list: &[String],
    ) -> Result<Option<Vec<StorageKey>>, ImageError> {
        let mut result = vec![];
        for digest in digest_list {
            if let Some(map) = self.local_blob(digest).await? {
                result.push(map);
            }
        }
        if result.len() == digest_list.len() {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn pull_blob(&mut self, image: &Reference, link: &Link) -> Result<Mmap, ImageError> {
        let key = StorageKey::Blob(link.digest.clone());
        let mmap = match self.storage.mmap(&key).await? {
            Some(map) => map,
            None => {
                let rc = self.registry_client_for(image).await?;
                log::info!("{} downloading {} bytes ...", image, link.size);
                let blob_data = rc.get_blob(&image.repository(), &link.digest).await?;
                // Note that the dkregistry library does verify the sha256 digest itself
                log::debug!("{} downloaded, {} bytes", link.digest, link.size);
                self.storage.insert(&key, &blob_data).await?;
                match self.storage.mmap(&key).await? {
                    Some(map) => map,
                    None => return Err(ImageError::StorageMissingAfterInsert),
                }
            } //
        };
        if mmap.len() as u64 == link.size {
            Ok(mmap)
        } else {
            // In the event the server gives us bad data, get_blob() should already
            // catch that during the digest verification. This path is more likely to hit
            // if the cached data on disk has been truncated.
            Err(ImageError::UnexpectedContentSize)
        }
    }

    async fn pull_runtime_config(
        &mut self,
        image: &Reference,
        link: &Link,
    ) -> Result<RuntimeConfig, ImageError> {
        if link.media_type == media_types::RUNTIME_CONFIG {
            let mapref = self.pull_blob(image, link).await?;
            let slice = &mapref[..];
            log::trace!(
                "raw json runtime config, {}",
                String::from_utf8_lossy(slice)
            );
            Ok(serde_json::from_slice(slice)?)
        } else {
            Err(ImageError::UnsupportedRuntimeConfigType(
                link.media_type.clone(),
            ))
        }
    }

    async fn decompress_layer(&mut self, data: Mmap) -> Result<(), ImageError> {
        let data_len = data.len();
        let output: Result<(StorageKey, Vec<u8>), ImageError> = task::spawn_blocking(move || {
            if data.len() > 1024 * 1024 {
                log::info!("decompressing {} bytes ...", data.len());
            }
            let mut decoder = GzDecoder::new(&data[..]);
            let mut output = vec![];
            decoder.read_to_end(&mut output)?;
            let key = StorageKey::from_blob_data(&output);
            Ok((key, output))
        })
        .await?;
        let (key, output) = output?;
        log::debug!(
            "decompressed {} bytes into {} bytes, {:?}",
            data_len,
            output.len(),
            key
        );
        self.storage.insert(&key, &output).await?;
        Ok(())
    }

    async fn pull_and_decompress_layers(
        &mut self,
        image: &Reference,
        manifest: &Manifest,
    ) -> Result<(), ImageError> {
        let mut tasks = FuturesUnordered::new();
        for link in &manifest.layers {
            let link = link.clone();
            let image = image.clone();
            let mut client = self.clone();
            tasks.push(task::spawn(async move {
                if link.media_type == media_types::LAYER_TAR_GZIP {
                    let tar_gzip = client.pull_blob(&image, &link).await?;
                    client.decompress_layer(tar_gzip).await
                } else {
                    Err(ImageError::UnsupportedLayerType(link.media_type))
                }
            }));
        }
        for result in tasks.next().await {
            println!("{:?}", result);
            result??;
        }
        Ok(())
    }

    pub async fn pull(&mut self, image: &Reference) -> Result<Arc<Image>, ImageError> {
        let manifest = self.pull_manifest(image).await?;
        let config = self.pull_runtime_config(image, &manifest.config).await?;

        // The manifest includes a list of compressed layers, which we will need to do
        // the download, but the content IDs we are really trying to follow are
        // the digests of the decompressed rootfs, since those come from the
        // runtime_config which has been verified by digest.

        if &config.rootfs.fs_type != FS_TYPE {
            Err(ImageError::UnsupportedRootFilesystemType(
                config.rootfs.fs_type.clone(),
            ))?;
        }
        let ids = &config.rootfs.diff_ids;

        let content = match self.local_blob_list(ids).await? {
            Some(content) => Ok(content),
            None => {
                self.pull_and_decompress_layers(image, &manifest).await?;
                match self.local_blob_list(ids).await? {
                    Some(content) => Ok(content),
                    None => Err(ImageError::UnexpectedDecompressedLayerContent),
                }
            }
        }?;

        let mut filesystem = vfs::Filesystem::new();
        let storage = self.storage.clone();

        for layer in &content {
            tar::extract(&mut filesystem, &storage, layer).await?;
        }

        Ok(Arc::new(Image {
            digest: manifest.config.digest,
            config,
            filesystem,
            storage,
        }))
    }
}
