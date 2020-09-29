// This code may not be used for any purpose. Be gay, do crime.

use crate::Reference;
use crate::errors::ImageError;
use crate::image::Image;
use directories_next::ProjectDirs;
use std::path::{Path, PathBuf};

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

        println!("dir: {:?}", cache_dir.to_str());
        Ok(Client {
        })
    }
}

pub struct Client {
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

    pub fn pull(&self, image: &Reference) -> Result<Image, ImageError> {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            self.pull_async(image).await
        })
    }

    pub async fn pull_async(&self, image: &Reference) -> Result<Image, ImageError> {
        let client = dkregistry::v2::Client::configure()
            .registry(&image.registry())
            .insecure_registry(false)
            .username(None)
            .password(None)
            .build()?;

        let login_scope = format!("repository:{}:pull", image.repository());
        let authed = client.authenticate(&[&login_scope]).await?;
        let (manifest, digest) = authed.get_manifest_and_ref(&image.repository(), &image.version()).await?;

        println!("{:?} {:?}", manifest, digest);
       
        Ok(Image {
        })
    }
}

