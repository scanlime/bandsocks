// This code may not be used for any purpose. Be gay, do crime.

use crate::Reference;
use crate::errors::ImageError;
use crate::image::Image;
use crate::manifest::{Manifest, RuntimeConfig, Link, media_types};
use crate::storage::{FileStorage, StorageKey};

use directories_next::ProjectDirs;
use dkregistry::v2::Client as RegistryClient;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::io::Read;
use memmap::Mmap;
use flate2::read::GzDecoder;

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
            None => ClientBuilder::default_cache_dir()?
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
        ClientBuilder {
            cache_dir: None,
        }
    }

    async fn registry_client_for<'a>(&'a mut self, image: &Reference) -> Result<&'a RegistryClient, ImageError> {
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

    pub fn pull(&mut self, image: &Reference) -> Result<Image, ImageError> {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            self.pull_async(image).await
        })
    }

    async fn pull_manifest(&mut self, image: &Reference) -> Result<Manifest, ImageError> {
        let key = StorageKey::Manifest(image.clone());
        let mmap = match self.storage.get(&key)? {
            Some(arc) => Ok(arc),
            None => {
                let rc = self.registry_client_for(image).await?;
                match rc.get_manifest(&image.repository(), &image.version()).await? {
                    dkregistry::v2::manifest::Manifest::S2(schema) => {
                        let spec_data = serde_json::to_vec(&schema.manifest_spec)?;
                        self.storage.insert(&key, spec_data).await
                    }
                    _ => Err(ImageError::UnsupportedManifestType)
                }
            }
        }?;
        let slice = &mmap[..];
        log::debug!("raw json manifest, {}", String::from_utf8_lossy(slice));
        Ok(serde_json::from_slice(slice)?)
    }

    fn local_blob(&mut self, digest: &str) -> Result<Option<Arc<Mmap>>, ImageError> {
        let key = StorageKey::Blob(digest.to_string());
        self.storage.get(&key)
    }

    fn local_blob_list(&mut self, digest_list: &[String]) -> Result<Option<Vec<Arc<Mmap>>>, ImageError> {
        let mut result = vec![];
        for digest in digest_list {            
            if let Some(mmap) = self.local_blob(digest)? {
                result.push(mmap);
            }
        }
        if result.len() == digest_list.len() {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn pull_blob(&mut self, image: &Reference, link: &Link) -> Result<Arc<Mmap>, ImageError> {
        let key = StorageKey::Blob(link.digest.clone());       
        let mmap = match self.storage.get(&key)? {
            Some(mmap) => mmap,
            None => {
                let rc = self.registry_client_for(image).await?;
                let blob_data = rc.get_blob(&image.repository(), &link.digest).await?;
                log::info!("{} downloaded, {} bytes", link.digest, link.size);
                self.storage.insert(&key, blob_data).await?
            }
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
    
    async fn pull_runtime_config(&mut self, image: &Reference, link: &Link) -> Result<RuntimeConfig, ImageError> {
        if link.media_type == media_types::RUNTIME_CONFIG {
            let mmap = self.pull_blob(image, link).await?;
            let slice = &mmap[..];
            log::debug!("raw json runtime config, {}", String::from_utf8_lossy(slice));
            Ok(serde_json::from_slice(slice)?)
        } else {
            Err(ImageError::UnsupportedRuntimeConfigType(link.media_type.clone()))
        }
    }

    async fn decompress_layer(&mut self, data: &[u8]) -> Result<(), ImageError> {
        let mut decoder = GzDecoder::new(data);
        let mut output = vec![];
        decoder.read_to_end(&mut output)?;
        let key = StorageKey::from_blob_data(&output);
        log::info!("decompressed {} bytes into {} bytes, {:?}", data.len(), output.len(), key);
        self.storage.insert(&key, output).await?;
        Ok(())
    }
    
    pub async fn pull_async(&mut self, image: &Reference) -> Result<Image, ImageError> {
        let manifest = self.pull_manifest(image).await?;
        let config = self.pull_runtime_config(image, &manifest.config).await?;

        // The manifest includes a list of compressed layers, which we will need to do the download,
        // but the content IDs we are really trying to follow are the digests of the decompressed rootfs,
        // since those come from the runtime_config which has been verified by digest.

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
        Ok(Image { config, content })
    }
}
