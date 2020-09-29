// This code may not be used for any purpose. Be gay, do crime.

use crate::Reference;
use crate::errors::ImageError;
use tokio::runtime::Runtime;
use dkregistry::v2::Client;

pub struct CacheBuilder {

}

impl CacheBuilder {
    pub fn new() -> CacheBuilder {
        CacheBuilder {
        }
    }

    pub fn build(self) -> Cache {
        Cache {
        }
    }
}
    
pub struct Cache {
}

#[derive(Debug)]
pub struct Image {
}

impl Cache {
    pub async fn pull_async(&self, image: &Reference) -> Result<Image, ImageError> {

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
       
        Ok(Image {
        })
    }

    pub fn pull(&self, image: &Reference) -> Result<Image, ImageError> {
        Runtime::new().unwrap().block_on(async { self.pull_async(image).await })
    }
}
