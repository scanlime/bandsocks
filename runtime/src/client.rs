// This code may not be used for any purpose. Be gay, do crime.

use crate::Reference;
use crate::errors::ImageError;
use crate::image::Image;
use crate::storage::{FileStorage, StorageKey};
use directories_next::ProjectDirs;
use dkregistry::v2::Client as RegistryClient;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use memmap::Mmap;

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
            Some((prev_image, client)) => {
                prev_image.registry() == image.registry()
                    && prev_image.repository() == image.repository()
            }
        };

        if !is_reusable {
            let client = dkregistry::v2::Client::configure()
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

    async fn cached_manifest(&mut self, image: &Reference) -> Result<Arc<Mmap>, ImageError> {
        let key = StorageKey::Manifest(image.clone());
        match self.storage.get(&key)? {
            Some(arc) => Ok(arc),
            None => {
                let rc = self.registry_client_for(image).await?;
                let manifest = rc.get_manifest(&image.repository(), &image.version()).await?;
                let data = b"bbb".to_vec();
                self.storage.insert(&key, data).await
            }
        }
    }
    
    pub async fn pull_async(&mut self, image: &Reference) -> Result<Image, ImageError> {
        let manifest = self.cached_manifest(image).await?;

        Ok(Image {
        })
    }
}

