// This code may not be used for any purpose. Be gay, do crime.

#[cfg(any(target_os="android", target_os="linux"))]
pub mod linux;

pub use dkregistry::reference::Reference;

use tokio::runtime::Runtime;
use dkregistry::v2::Client;
use thiserror::Error;
use std::fmt;

#[derive(Error, Debug)]
pub enum ImageError {
    Registry {
        #[from]
        source: dkregistry::errors::Error
    }
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Registry { source } => write!(f, "registry error: {}", source),
        }
    }
}

pub struct ImageCache {
}

#[derive(Debug)]
pub struct ImageData {
}

impl ImageCache {
    pub fn new() -> ImageCache {
        ImageCache {
        }
    }

    pub async fn pull(&self, image: &Reference) -> Result<ImageData, ImageError> {

        let client = Client::configure()
            .registry(&image.registry())
            .insecure_registry(false)
            .username(None)
            .password(None)
            .build()?;

        let login_scope = format!("repository:{}:pull", image.repository());
        let authed = client.authenticate(&[&login_scope]).await?;
        let (manifest, digest) = authed.get_manifest_and_ref(&image.repository(), &image.version()).await?;

        println!("{:?} {:?}", manifest, digest);
       
        Ok(ImageData {
        })
    }

    pub fn pull_sync(&self, image: &Reference) -> Result<ImageData, ImageError> {
        Runtime::new().unwrap().block_on(async { self.pull(image).await })
    }
}
