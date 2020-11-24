//! Support for downloading container images from a registry server

use crate::image::{ImageName, Registry, Repository};

/// Additional settings for compatibility with a default registry server
///
/// If you don't need the additional options, you can convert a plain [Registry]
/// [Into] a [DefaultRegistry]
#[derive(Clone, Debug)]
pub struct DefaultRegistry {
    /// Connect to the registry under this name
    pub network_name: Registry,
    /// This registry is also known under additional names
    pub also_known_as: Vec<Registry>,
    /// Use this prefix when accessing an image repository with only a single
    /// path component
    pub library_prefix: Option<Repository>,
}

impl From<Registry> for DefaultRegistry {
    fn from(network_name: Registry) -> Self {
        DefaultRegistry {
            network_name,
            also_known_as: vec![],
            library_prefix: None,
        }
    }
}

impl DefaultRegistry {
    /// Return the built-in defaults
    pub fn new() -> Self {
        DefaultRegistry {
            network_name: "registry-1.docker.io".parse().unwrap(),
            also_known_as: vec!["docker.io".parse().unwrap()],
            library_prefix: Some("library".parse().unwrap()),
        }
    }

    /// Check whether a particular registry is considered default under these
    /// settings
    ///
    /// Returns true if the given registry is None or if it matches either the
    /// `network_name` or any of the `also_known_as` settings here.
    pub fn is_default(&self, registry: &Option<Registry>) -> bool {
        match registry {
            None => true,
            Some(registry) => {
                registry == &self.network_name || self.also_known_as.contains(registry)
            }
        }
    }

    /// Use these settings to determine the actual network server and path for
    /// an image
    pub fn resolve_image_name(&self, image: &ImageName) -> (Registry, Repository) {
        let registry = image.registry();
        let settings = if self.is_default(&registry) {
            self.clone()
        } else {
            registry.unwrap().clone().into()
        };

        let image_repo = image.repository();
        let complete_repo = if image_repo.iter().nth(1).is_some() {
            image_repo
        } else {
            match &settings.library_prefix {
                None => image_repo,
                Some(prefix) => prefix.join(&image_repo),
            }
        };

        (settings.network_name, complete_repo)
    }
}
