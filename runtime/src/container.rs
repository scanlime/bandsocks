// This code may not be used for any purpose. Be gay, do crime.

use crate::Reference;
use crate::image::Image;
use crate::client::Client;
use crate::filesystem::vfs::Filesystem;
use crate::errors::{ImageError, RuntimeError};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::ffi::{OsStr, OsString};
use std::default::Default;

#[derive(Default)]
pub struct ContainerBuilder {
    image: Option<Arc<Image>>,
    arg_list: Vec<OsString>,
    env_list: Vec<EnvBuilder>,
    current_dir: Option<OsString>,
}

enum EnvBuilder {
    Set(OsString, OsString),
    Remove(OsString),
    Clear
}

impl ContainerBuilder {
    pub fn spawn(&self) -> Result<Container, RuntimeError> {
        // it might be nice to enforce this at compile-time instead... right now it seemed
        // worth allowing for multiple ways to load images without the types getting too complex.
        let image = match &self.image {
            None => Err(RuntimeError::NoImage)?,
            Some(image) => image.clone()
        };

        // this is a shallow copy of the image's reference filesystem, which the container can modify
        let filesystem = image.filesystem.clone();

        let mut dir = PathBuf::new();
        dir.push(&image.config.config.working_dir);
        if let Some(dir_override) = &self.current_dir {
            dir.push(dir_override);
        }

        let mut env = BTreeMap::new();
        for configured_env in &image.config.config.env {
            let mut iter = configured_env.splitn(2, "=");
            if let Some(key) = iter.next() {
                let value = match iter.next() {
                    Some(value) => value,
                    None => "",
                };
                env.insert(OsString::from(key), OsString::from(value));
            }
        }
        for env_override in &self.env_list {
            match env_override {
                EnvBuilder::Clear => { env.clear(); },
                EnvBuilder::Remove(key) => { env.remove(key); },
                EnvBuilder::Set(key, value) => { env.insert(key.clone(), value.clone()); },
            }
        }        
        
        log::info!("running container,");
        log::info!("using image: {}", image.digest);
        log::info!("args: {:?}", self.arg_list);
        log::info!("cmd: {:?}", image.config.config.cmd);
        log::info!("entrypoint: {:?}", image.config.config.entrypoint);
        log::info!("env: {:?}", env);
        log::info!("dir: {:?}", dir);

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

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.current_dir = Some(dir.as_ref().as_os_str().to_os_string());
        self
    }
    
    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env_list.push(EnvBuilder::Set(key.as_ref().to_os_string(),
                                           val.as_ref().to_os_string()));
        self            
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (ref key, ref val) in vars {
            self.env(key, val);
        }
        self
    }

    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Self {
        self.env_list.push(EnvBuilder::Remove(key.as_ref().to_os_string()));
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self.env_list.push(EnvBuilder::Clear);
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

    pub fn pull(image_reference: &Reference) -> Result<ContainerBuilder, ImageError> {
        let mut builder = Container::new();
        builder.image(&Client::new()?.pull(image_reference)?);
        Ok(builder)
    }
}
