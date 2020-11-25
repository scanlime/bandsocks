//! Support for downloading container images from a registry server

use crate::{
    errors::ImageError,
    filesystem::{
        storage::{FileStorage, StorageKey},
        tar, vfs,
    },
    image::{ContentDigest, Image, ImageName, Registry, Repository},
    manifest::{media_types, Link, Manifest, RuntimeConfig, FS_TYPE},
    registry::{auth::Auth, DefaultRegistry},
};

use async_compression::tokio_02::write::GzipDecoder;
use futures_util::{stream::FuturesUnordered, StreamExt};
use http::header::HeaderValue;
use memmap::Mmap;
use reqwest::{header, header::HeaderMap, Certificate, RequestBuilder, StatusCode};
use std::{
    collections::HashSet,
    convert::TryInto,
    env,
    fmt::Display,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, task};
use url::Url;

/// Builder for configuring custom [Client] instances
pub struct ClientBuilder {
    req: reqwest::ClientBuilder,
    auth: Auth,
    cache_dir: Option<PathBuf>,
    default_registry: Option<DefaultRegistry>,
    allowed_registries: Option<HashSet<Registry>>,
    allow_http_registries: bool,
}

impl ClientBuilder {
    /// Start constructing a custom registry client
    pub fn new() -> Self {
        let req = reqwest::Client::builder().user_agent(Client::default_user_agent());
        ClientBuilder {
            req,
            cache_dir: None,
            default_registry: None,
            auth: Auth::new(),
            allowed_registries: None,
            allow_http_registries: true,
        }
    }

    /// Disallow connecting to registries via HTTP
    ///
    /// The way Docker parses image names, values like `localhost/blah` or
    /// `dev:5000/foo` will be interpreted as hosts to contact over unencrypted
    /// HTTP. This setting disallows such registries.
    pub fn disallow_http(mut self) -> Self {
        self.allow_http_registries = false;
        self
    }

    /// Set a list of allowed registry servers
    ///
    /// All connections will be checked against this list. The default registry
    /// is not automatically added to the list. If no allowed registry list is
    /// set, any server will be allowed. An empty allow list will disallow all
    /// connections, but the local cache will still be used if available.
    pub fn allow_only_connections_to(mut self, allowed: HashSet<Registry>) -> Self {
        self.allowed_registries = Some(allowed);
        self
    }

    /// Change the cache directory
    ///
    /// This stores local data which has been downloaded and/or decompressed.
    /// Files here are read-only after they are created, and may be shared
    /// with other trusted processes. The default directory can be determined
    /// with [Client::default_cache_dir()]
    pub fn cache_dir(mut self, dir: &Path) -> Self {
        self.cache_dir = Some(dir.to_path_buf());
        self
    }

    /// Set a random, disposable cache directory
    ///
    /// This is currently equivalent to calling cache_dir() on a randomly
    /// generated path in the system temp directory. In the future this
    /// setting may enable an entirely in-memory storage backend.
    pub fn ephemeral_cache(self) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "bandsocks-ephemeral-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        self.cache_dir(&path)
    }

    /// Set a timeout for each network request
    ///
    /// This timeout applies from the beginning of a (GET) request until the
    /// last byte has been received. By default there is no timeout.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.req = self.req.timeout(timeout);
        self
    }

    /// Set a timeout for only the initial connect phase of each network request
    ///
    /// By default there is no timeout beyond those built into the networking
    /// stack.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.req = self.req.connect_timeout(timeout);
        self
    }

    /// Sets the `User-Agent` header used by this client
    ///
    /// By default, the value returened by [Client::default_user_agent()] is
    /// used, which identifies the version of `bandsocks` acting as a
    /// client.
    pub fn user_agent<V>(mut self, value: V) -> Self
    where
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        self.req = self.req.user_agent(value);
        self
    }

    /// Bind to a specific local IP address
    pub fn local_address<T>(mut self, addr: T) -> Self
    where
        T: Into<Option<std::net::IpAddr>>,
    {
        self.req = self.req.local_address(addr);
        self
    }

    /// Set the default headers for every HTTP request
    pub fn default_request_headers(mut self, headers: HeaderMap) -> Self {
        self.req = self.req.default_headers(headers);
        self
    }

    /// Trust an additional root certificate
    pub fn add_root_certificate(mut self, certificate: Certificate) -> Self {
        self.req = self.req.add_root_certificate(certificate);
        self
    }

    /// Change the default registry server
    ///
    /// This registry is used for pulling images that do not specify a server.
    /// The default value if unset can be determined with
    /// [Client::default_registry()]
    ///
    /// The parameter is a [DefaultRegistry], which provides a few additional
    /// options for emulating registry quirks. When those aren't needed,
    /// a [Registry] can be converted directly into a [DefaultRegistry] by
    /// calling its `into()`.
    pub fn registry(mut self, default_registry: &DefaultRegistry) -> Self {
        self.default_registry = Some(default_registry.clone());
        self
    }

    /// Store a username and password for use with a particular registry on this
    /// client
    pub fn login(mut self, registry: Registry, username: String, password: String) -> Self {
        self.auth.login(registry, username, password);
        self
    }

    /// Construct a Client using the parameters from this Builder
    pub fn build(self) -> Result<Client, ImageError> {
        let cache_dir = match self.cache_dir {
            Some(dir) => dir,
            None => Client::default_cache_dir()?,
        };
        log::info!("using cache directory {:?}", cache_dir);
        Ok(Client {
            storage: FileStorage::new(cache_dir),
            allowed_registries: self.allowed_registries,
            allow_http_registries: self.allow_http_registries,
            default_registry: self
                .default_registry
                .unwrap_or_else(Client::default_registry),
            auth: self.auth,
            req: self.req.build()?,
        })
    }
}

/// Registry clients can download and store data from an image registry
///
/// Each client includes settings like authentication, default server, and a
/// cache storage location. One client can be used to download multiple images
/// from multiple registries.
#[derive(Clone)]
pub struct Client {
    storage: FileStorage,
    auth: Auth,
    req: reqwest::Client,
    default_registry: DefaultRegistry,
    allowed_registries: Option<HashSet<Registry>>,
    allow_http_registries: bool,
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

    /// Return the default `User-Agent` that we use if no other is set
    pub fn default_user_agent() -> HeaderValue {
        static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
        HeaderValue::from_static(USER_AGENT)
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
    /// [ImageName] or [ClientBuilder].
    pub fn default_registry() -> DefaultRegistry {
        DefaultRegistry::new()
    }

    fn is_registry_allowed(&self, registry: &Registry) -> bool {
        (self.allow_http_registries || registry.is_https())
            && match &self.allowed_registries {
                None => true,
                Some(allow_list) => allow_list.contains(registry),
            }
    }

    fn verify_registry_allowed(&self, registry: &Registry) -> Result<(), ImageError> {
        if self.is_registry_allowed(registry) {
            Ok(())
        } else {
            log::warn!("registry {} not allowed by configuration", registry);
            Err(ImageError::RegistryNotAllowed(registry.clone()))
        }
    }

    fn build_v2_url<T>(
        &self,
        registry: &Registry,
        repository: &Repository,
        bucket: &'static str,
        object: T,
    ) -> Result<Url, ImageError>
    where
        T: Display,
    {
        self.verify_registry_allowed(registry)?;
        Ok(format!(
            "{}://{}/v2/{}/{}/{}",
            registry.protocol_str(),
            registry,
            repository,
            bucket,
            object
        )
        .parse()
        .expect("url components already validated"))
    }

    async fn download_to_storage<F, V>(
        &mut self,
        registry: &Registry,
        from_req: RequestBuilder,
        to_key: &StorageKey,
        validator: F,
    ) -> Result<V, ImageError>
    where
        F: FnOnce(ContentDigest) -> Result<V, ImageError>,
    {
        let response = {
            let from_req_copy = from_req.try_clone().unwrap();
            let response = from_req_copy.send().await?;
            if response.status() == StatusCode::UNAUTHORIZED {
                match response.headers().get(reqwest::header::WWW_AUTHENTICATE).map(|h| h.to_str()) {
                    None => response,
                    Some(Err(_bad_string)) => response,
                    Some(Ok(auth_header)) => {
                        self.auth.authenticate_for(registry, &self.req, auth_header).await?;
                        from_req.send().await?
                    }
                }
            } else {
                response
            }
        };

        log::info!("downloading {}", response.url());
        let mut response = response.error_for_status()?;
        let mut writer = self.storage.begin_write().await?;

        let result: Result<(), ImageError> = loop {
            match response.chunk().await {
                Err(err) => break Err(err.into()),
                Ok(None) => break Ok(()),
                Ok(Some(chunk)) => match writer.write_all(&chunk).await {
                    Err(err) => break Err(err.into()),
                    Ok(()) => (),
                },
            }
        };
        if let Err(err) = result {
            writer.remove_temp().await?;
            return Err(err.into());
        }
        let content_digest = match writer.finalize().await {
            Ok(content_digest) => content_digest,
            Err(err) => {
                writer.remove_temp().await?;
                return Err(err.into());
            }
        };
        match validator(content_digest) {
            Ok(validated) => {
                self.storage.commit_write(writer, to_key).await?;
                log::debug!("stored {:?}", to_key);
                Ok(validated)
            }
            Err(err) => {
                writer.remove_temp().await?;
                return Err(err.into());
            }
        }
    }

    async fn pull_manifest(
        &mut self,
        image: &ImageName,
    ) -> Result<(ImageName, Manifest), ImageError> {
        let (registry, repository) = self.default_registry.resolve_image_name(image);
        let key = StorageKey::Manifest(registry, repository, image.version());

        let (specific_image, map) = match self.storage.mmap(&key).await? {
            Some(map) => {
                // If the manifest is cached, still verify its content digest and annotate the
                // ImageName with that digest
                let found_digest = ContentDigest::from_content(&map[..]);
                let specific_image = image.with_found_digest(&found_digest)?;
                log::debug!("{} manifest in cache is good", specific_image);
                (specific_image, map)
            }
            None => match &key {
                StorageKey::Manifest(registry, repository, version) => {
                    if !(registry.is_https() || image.content_digest_str().is_some()) {
                        return Err(ImageError::InsecureManifest);
                    }

                    let url = self.build_v2_url(&registry, &repository, "manifests", &version)?;
                    let request = self
                        .req
                        .get(url)
                        .header(header::ACCEPT, media_types::MANIFEST);

                    // Stream the download to a temp file while hashing it, verify the digest, and
                    // commits the file to storage only if the validator passes
                    let specific_image = self
                        .download_to_storage(&registry, request, &key, |content_digest| {
                            image.with_found_digest(&content_digest)
                        })
                        .await?;

                    // If the specific name is different than the one it was requested under, the
                    // image was requested by tag but now the digest is known. Make a copy of the
                    // manifest under its more specific name.
                    if &specific_image != image {
                        let specific_key = StorageKey::Manifest(
                            registry.clone(),
                            repository.clone(),
                            specific_image.version(),
                        );
                        self.storage.copy_data(&key, &specific_key).await?;
                    }

                    let map = match self.storage.mmap(&key).await? {
                        Some(map) => map,
                        None => return Err(ImageError::StorageMissingAfterInsert),
                    };

                    (specific_image, map)
                }
                _ => unreachable!(),
            },
        };

        let slice = &map[..];
        log::trace!(
            "raw json manifest for {}: {:?}",
            specific_image,
            String::from_utf8_lossy(slice)
        );
        Ok((specific_image, serde_json::from_slice(slice)?))
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
        let (registry, repository) = self.default_registry.resolve_image_name(image);
        let key = StorageKey::Blob(ContentDigest::parse(&link.digest)?);
        let content_type = HeaderValue::from_str(&link.media_type)
            .map_err(|_| ImageError::InvalidContentType(link.media_type.clone()))?;

        let map = match self.storage.mmap(&key).await? {
            Some(map) => {
                log::debug!("{} blob {} is already cached", image, link.digest);
                map
            }
            None => match &key {
                StorageKey::Blob(content_digest) => {
                    let url =
                        self.build_v2_url(&registry, &repository, "blobs", &content_digest)?;
                    let request = self.req.get(url).header(header::ACCEPT, content_type);

                    self.download_to_storage(&registry, request, &key, |found_digest| {
                        if &found_digest == content_digest {
                            Ok(())
                        } else {
                            Err(ImageError::ContentDigestMismatch {
                                expected: content_digest.clone(),
                                found: found_digest,
                            })
                        }
                    })
                    .await?;

                    match self.storage.mmap(&key).await? {
                        Some(map) => map,
                        None => return Err(ImageError::StorageMissingAfterInsert),
                    }
                }
                _ => unreachable!(),
            },
        };

        if map.len() as u64 == link.size {
            Ok(map)
        } else {
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
        if data.len() >= 500000 {
            log::info!("decompressing {} bytes", data.len());
        }

        let mut writer = self.storage.begin_write().await?;
        let mut decoder = GzipDecoder::new(&mut writer);
        let result = decoder.write_all(&data[..]).await;
        let result = match result {
            Ok(()) => decoder.shutdown().await,
            Err(err) => Err(err),
        };
        match result {
            Err(err) => {
                writer.remove_temp().await?;
                Err(err.into())
            }
            Ok(()) => {
                let content_digest = writer.finalize().await?;
                let key = StorageKey::Blob(content_digest);
                log::debug!("decompressed {} bytes into {:?}", data.len(), key);
                self.storage.commit_write(writer, &key).await?;
                Ok(())
            }
        }
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
                    // we don't actually need to keep the compressed blob, after verifying it and
                    // decompressing it. but doing it this way is a little easier, and it lets us
                    // cleanly restart just the decompression part if we got terminated after a
                    // download finished but before it was unpacked.
                    let tar_gzip = client.pull_blob(&image, &link).await?;
                    client.decompress_layer(tar_gzip).await
                } else {
                    Err(ImageError::UnsupportedLayerType(link.media_type))
                }
            }));
        }
        for result in tasks.next().await {
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
        let (specific_image, manifest) = self.pull_manifest(image).await?;
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

        Ok(Arc::new(Image {
            name: specific_image,
            config,
            filesystem,
            storage,
        }))
    }
}
