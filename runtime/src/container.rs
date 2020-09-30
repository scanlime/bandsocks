// This code may not be used for any purpose. Be gay, do crime.

use crate::Reference;
use crate::image::Image;
use crate::client::Client;
use crate::filesystem::vfs::Filesystem;
use crate::errors::{ImageError, RuntimeError};
use std::sync::Arc;
use std::ffi::{OsStr, OsString};
use std::default::Default;

#[derive(Default)]
pub struct ContainerBuilder {
    image: Option<Arc<Image>>,
    arg_list: Vec<OsString>,
}

impl ContainerBuilder {
    pub fn spawn(&self) -> Result<Container, RuntimeError> {
        let image = match &self.image {
            None => Err(RuntimeError::NoImage)?,
            Some(image) => image.clone()
        };

        log::info!("running container with image {}, args: {:?}",
                   image.digest, self.arg_list);

        // this is a shallow copy of the image's reference filesystem, which the container can modify
        let filesystem = image.filesystem.clone();
        
        Ok(Container {
            image,
            filesystem
        })
    }

    pub fn image(&mut self, image: &Arc<Image>) -> &mut Self {
        self.image = Some(image.clone());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg.as_ref());
        }
        self
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.arg_list.push(arg.as_ref().to_os_string());
        self
    }
}

#[derive(Debug)]
pub struct Container {
    image: Arc<Image>,
    filesystem: Filesystem
}

impl Container {
    pub fn new() -> ContainerBuilder {
        Default::default()
    }

    /// High-level convenience function to create a client and pull the indicated image
    pub fn pull(image_reference: &Reference) -> Result<ContainerBuilder, ImageError> {
        let mut builder = Container::new();
        builder.image(&Client::new()?.pull(image_reference)?);
        Ok(builder)
    }
}
