//! Optional lower-level interface to the container registry and cache

use crate::{
    errors::ImageError,
    filesystem::{
        storage::{FileStorage, StorageKey},
        tar, vfs,
    },
    image::{ContentDigest, Image, ImageName, Registry},
    manifest::{media_types, Link, Manifest, RuntimeConfig, FS_TYPE},
};

use flate2::read::GzDecoder;
use futures_util::{stream::FuturesUnordered, StreamExt};
use memmap::Mmap;
use std::{
    collections::HashMap,
    env,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::task;

/// Builder for configuring custom [Client] instances
#[derive(Clone)]
pub struct ClientBuilder {
    cache_dir: Option<PathBuf>,
    registry: Option<Registry>,
    logins: HashMap<Registry, (String, String)>,
}

impl ClientBuilder {
    /// Start constructing a custom registry client
    pub fn new() -> Self {
        ClientBuilder {
            cache_dir: None,
            registry: None,
            logins: HashMap::new(),
        }
    }

    /// Change the cache directory
    ///
    /// This stores local data which has been downloaded and/or decompressed.
    /// Files here are read-only after they are created, and may be shared
    /// with other trusted processes. The default directory can be determined
    /// with [Client::default_cache_dir()]
    pub fn cache_dir(&mut self, dir: &Path) -> &mut Self {
        self.cache_dir = Some(dir.to_path_buf());
        self
    }

    /// Change the default registry server
    ///
    /// This registry is used for pulling images that do not specify a server.
    /// The default value if unset can be determined with
    /// [Client::default_registry()]
    pub fn registry(&mut self, registry: &Registry) -> &mut Self {
        self.registry = Some(registry.clone());
        self
    }

    /// Store a username and password for use with a particular registry on this
    /// client
    pub fn login(&mut self, registry: Registry, username: String, password: String) -> &mut Self {
        self.logins.insert(registry, (username, password));
        self
    }

    /// Construct a Client using the parameters from this Builder
    pub fn build(self) -> Result<Client, ImageError> {
        let cache_dir = match self.cache_dir {
            Some(dir) => dir,
            None => Client::default_cache_dir()?,
        };
        log::info!("using cache directory {:?}", cache_dir);

        static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
        let req = reqwest::Client::builder().user_agent(USER_AGENT).build()?;

        Ok(Client {
            storage: FileStorage::new(cache_dir),
            registry: self.registry.unwrap_or_else(Client::default_registry),
            logins: self.logins,
            req,
        })
    }
}

/// Registry clients can download and store data from an image registry
#[derive(Clone)]
pub struct Client {
    storage: FileStorage,
    registry: Registry,
    logins: HashMap<Registry, (String, String)>,
    req: reqwest::Client,
}

impl Client {
    /// Construct a new registry client with default options
    pub fn new() -> Result<Client, ImageError> {
        Client::builder().build()
    }

    /// Construct a registry client with custom options, via ClientBuilder
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Determine a default per-user cache directory which will be used if an
    /// alternate cache directory is not specified.
    ///
    /// Typically this returns `$HOME/.cache/bandsocks`, but it may return
    /// `$XDG_CACHE_HOME/bandsocks` if the per-user cache directory has been
    /// set, and the default cache location can be customized directly via the
    /// `$BANDSOCKS_CACHE` environment variable.
    pub fn default_cache_dir() -> Result<PathBuf, ImageError> {
        match env::var("BANDSOCKS_CACHE") {
            Ok(s) => Ok(Path::new(&s).to_path_buf()),
            Err(_) => {
                let mut buf = match env::var("XDG_CACHE_HOME") {
                    Ok(s) => Ok(Path::new(&s).to_path_buf()),
                    Err(_) => match env::var("HOME") {
                        Ok(s) => Ok(Path::new(&s).join(".cache")),
                        Err(_) => Err(ImageError::NoDefaultCacheDir),
                    },
                };
                if let Ok(buf) = &mut buf {
                    buf.push("bandsocks");
                }
                buf
            }
        }
    }

    /// Return the default registry server
    ///
    /// This is the server used when nothing else has been specified either in
    /// [ImageName] or [ClientBuilder]. Currently this always returns
    /// `registry-1.docker.io`
    pub fn default_registry() -> Registry {
        Registry::parse("registry-1.docker.io").unwrap()
    }

    async fn pull_manifest(&mut self, image: &ImageName) -> Result<Manifest, ImageError> {
        let key = StorageKey::Manifest(image.clone());
        let map = match self.storage.mmap(&key).await? {
            Some(map) => map,
            None => {
                log::info!("{} downloading manifest...", image);

                let registry = image.registry().unwrap_or_else(|| self.registry.clone());
                let url = format!(
                    "{}://{}/v2/{}{}/manifests/{}",
                    registry.protocol(),
                    registry,
                    "", // library prefix
                    image.repository_str(),
                    image.digest_str().or(image.tag_str()).unwrap_or("latest")
                );

                log::debug!("URL {}", url);

                unimplemented!();
                /*
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
                */
            }
        };
        let slice = &map[..];
        log::trace!("raw json manifest, {}", String::from_utf8_lossy(slice));
        Ok(serde_json::from_slice(slice)?)
    }

    async fn storage_key_for_content_if_local(
        &mut self,
        digest: &ContentDigest,
    ) -> Result<Option<StorageKey>, ImageError> {
        let key = StorageKey::Blob(digest.clone());
        match self.storage.open(&key).await {
            Err(e) => Err(e),
            Ok(None) => Ok(None),
            Ok(_map) => Ok(Some(key)),
        }
    }

    async fn storage_keys_for_content_if_all_local(
        &mut self,
        digests: &Vec<String>,
    ) -> Result<Option<Vec<StorageKey>>, ImageError> {
        let mut result = vec![];
        for digest in digests {
            if let Some(map) = self
                .storage_key_for_content_if_local(&ContentDigest::parse(digest)?)
                .await?
            {
                result.push(map);
            } else {
                return Ok(None);
            }
        }
        Ok(Some(result))
    }

    async fn pull_blob(&mut self, image: &ImageName, link: &Link) -> Result<Mmap, ImageError> {
        let key = StorageKey::Blob(ContentDigest::parse(&link.digest)?);
        let mmap = match self.storage.mmap(&key).await? {
            Some(map) => map,
            None => {
                unimplemented!();
                /*
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
                */
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

    async fn pull_runtime_config(
        &mut self,
        image: &ImageName,
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
        image: &ImageName,
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

    /// Resolve an [ImageName] into an [Image] if possible
    ///
    /// This will always try to load the image from local cache first without
    /// accessing the network. If the image is not already available in cache,
    /// it will be downloaded from the indicated registry server. If a content
    /// digest is given, it will be verified and the image is only returned
    /// if it matches the expected content.
    ///
    /// The resulting image is mapped into memory and ready for use in any
    /// number of containers.
    pub async fn pull(&mut self, image: &ImageName) -> Result<Arc<Image>, ImageError> {
        let manifest = self.pull_manifest(image).await?;
        let config = self.pull_runtime_config(image, &manifest.config).await?;

        if &config.rootfs.fs_type != FS_TYPE {
            Err(ImageError::UnsupportedRootFilesystemType(
                config.rootfs.fs_type.clone(),
            ))?;
        }
        let ids = &config.rootfs.diff_ids;

        let content = match self.storage_keys_for_content_if_all_local(ids).await? {
            Some(content) => Ok(content),
            None => {
                self.pull_and_decompress_layers(image, &manifest).await?;
                match self.storage_keys_for_content_if_all_local(ids).await? {
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

        let digest = ContentDigest::parse("blah").unwrap();

        Ok(Arc::new(Image {
            name: image.with_specific_digest(&digest)?,
            config,
            filesystem,
            storage,
        }))
    }
}
