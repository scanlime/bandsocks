use crate::{
    errors::ImageError,
    filesystem::{mmap::MapRef, tar, vfs},
    image::Image,
    manifest::{media_types, Link, Manifest, RuntimeConfig, FS_TYPE},
    storage::{FileStorage, StorageKey},
    Reference,
};

use directories_next::ProjectDirs;
use dkregistry::v2::Client as RegistryClient;
use flate2::read::GzDecoder;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct ClientBuilder {
    cache_dir: Option<PathBuf>,
}

impl ClientBuilder {
    pub fn cache_dir(&mut self, dir: &Path) -> &mut Self {
        self.cache_dir = Some(dir.to_path_buf());
        self
    }

    fn default_cache_dir() -> Result<PathBuf, ImageError> {
        if let Some(proj_dirs) = ProjectDirs::from("org", "scanlime", "bandsocks") {
            Ok(proj_dirs.cache_dir().to_path_buf())
        } else {
            Err(ImageError::NoDefaultCacheDir)
        }
    }

    pub fn build(self) -> Result<Client, ImageError> {
        let cache_dir = match self.cache_dir {
            Some(dir) => dir,
            None => ClientBuilder::default_cache_dir()?,
        };
        Ok(Client {
            storage: FileStorage::new(cache_dir),
            registry_client: None,
        })
    }
}

pub struct Client {
    storage: FileStorage,
    registry_client: Option<(Reference, RegistryClient)>,
}

impl Client {
    pub fn new() -> Result<Client, ImageError> {
        Client::configure().build()
    }

    pub fn configure() -> ClientBuilder {
        ClientBuilder { cache_dir: None }
    }

    async fn registry_client_for<'a>(
        &'a mut self,
        image: &Reference,
    ) -> Result<&'a RegistryClient, ImageError> {
        let is_reusable = match &self.registry_client {
            None => false,
            Some((prev_image, _)) => {
                prev_image.registry() == image.registry()
                    && prev_image.repository() == image.repository()
            }
        };

        if !is_reusable {
            let client = RegistryClient::configure()
                .registry(&image.registry())
                .insecure_registry(false)
                .username(None)
                .password(None)
                .build()?;

            let login_scope = format!("repository:{}:pull", image.repository());
            let client = client.authenticate(&[&login_scope]).await?;
            self.registry_client.replace((image.clone(), client));
        }

        Ok(&self.registry_client.as_ref().unwrap().1)
    }

    async fn pull_manifest(&mut self, image: &Reference) -> Result<Manifest, ImageError> {
        let key = StorageKey::Manifest(image.clone());
        let map = match self.storage.get(&key)? {
            Some(map) => map,
            None => {
                let rc = self.registry_client_for(image).await?;
                match rc
                    .get_manifest(&image.repository(), &image.version())
                    .await?
                {
                    dkregistry::v2::manifest::Manifest::S2(schema) => {
                        let spec_data = serde_json::to_vec(&schema.manifest_spec)?;
                        self.storage.insert(&key, spec_data).await?
                    }
                    _ => Err(ImageError::UnsupportedManifestType)?,
                }
            }
        };
        let slice = &map[..];
        log::debug!("raw json manifest, {}", String::from_utf8_lossy(slice));
        Ok(serde_json::from_slice(slice)?)
    }

    fn local_blob(&mut self, digest: &str) -> Result<Option<MapRef>, ImageError> {
        let key = StorageKey::Blob(digest.to_string());
        self.storage.get(&key)
    }

    fn local_blob_list(
        &mut self,
        digest_list: &[String],
    ) -> Result<Option<Vec<MapRef>>, ImageError> {
        let mut result = vec![];
        for digest in digest_list {
            if let Some(mapref) = self.local_blob(digest)? {
                result.push(mapref);
            }
        }
        if result.len() == digest_list.len() {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn pull_blob(&mut self, image: &Reference, link: &Link) -> Result<MapRef, ImageError> {
        let key = StorageKey::Blob(link.digest.clone());
        let mapref = match self.storage.get(&key)? {
            Some(mapref) => mapref,
            None => {
                let rc = self.registry_client_for(image).await?;
                let blob_data = rc.get_blob(&image.repository(), &link.digest).await?;
                log::info!("{} downloaded, {} bytes", link.digest, link.size);
                self.storage.insert(&key, blob_data).await?
            }
        };
        if mapref.len() as u64 == link.size {
            Ok(mapref)
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
            log::debug!(
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

    async fn decompress_layer(&mut self, data: &[u8]) -> Result<(), ImageError> {
        let mut decoder = GzDecoder::new(data);
        let mut output = vec![];
        decoder.read_to_end(&mut output)?;
        let key = StorageKey::from_blob_data(&output);
        log::info!(
            "decompressed {} bytes into {} bytes, {:?}",
            data.len(),
            output.len(),
            key
        );
        self.storage.insert(&key, output).await?;
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
        let content = match self.local_blob_list(ids)? {
            Some(content) => Ok(content),
            None => {
                for link in &manifest.layers {
                    if link.media_type == media_types::LAYER_TAR_GZIP {
                        let tar_gzip = self.pull_blob(image, link).await?;
                        self.decompress_layer(&tar_gzip[..]).await?;
                    } else {
                        Err(ImageError::UnsupportedLayerType(link.media_type.clone()))?;
                    }
                }
                match self.local_blob_list(ids)? {
                    Some(content) => Ok(content),
                    None => Err(ImageError::UnexpectedDecompressedLayerContent),
                }
            }
        }?;

        let mut filesystem = vfs::Filesystem::new();
        for layer in &content {
            tar::extract_metadata(&mut filesystem, layer)?;
        }

        Ok(Arc::new(Image {
            digest: manifest.config.digest,
            config,
            filesystem,
        }))
    }
}
