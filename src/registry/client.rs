//! Support for downloading container images from a registry server

use crate::{
    errors::ImageError,
    filesystem::{
        storage,
        storage::{FileStorage, StorageKey, StorageWriter},
        tar, vfs,
    },
    image::{ContentDigest, Image, ImageName, ImageVersion, Registry, Repository},
    manifest::{media_types, Link, Manifest, RuntimeConfig, FS_TYPE},
    registry::{auth::Auth, DefaultRegistry},
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use memmap::Mmap;
use reqwest::{
    header,
    header::{HeaderMap, HeaderValue},
    Certificate, RequestBuilder, Url,
};
use std::{
    collections::HashSet,
    convert::TryInto,
    env,
    fmt::Display,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, task};

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
    pub fn login(mut self, registry: Registry, username: String, password: Option<String>) -> Self {
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
        storage::default_cache_dir()
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

    fn build_url<T>(
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

    async fn download(
        &mut self,
        registry: &Registry,
        from_req: RequestBuilder,
    ) -> Result<(StorageWriter, ContentDigest), ImageError> {
        let response = self.auth.request(registry, &self.req, from_req).await?;
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
        match result {
            Err(err) => {
                writer.remove_temp().await?;
                Err(err.into())
            }
            Ok(()) => match writer.finalize().await {
                Ok(content_digest) => {
                    log::debug!("download has digest {}", content_digest);
                    Ok((writer, content_digest))
                }
                Err(err) => {
                    writer.remove_temp().await?;
                    Err(err.into())
                }
            },
        }
    }

    async fn download_manifest(
        &mut self,
        registry: &Registry,
        repository: &Repository,
        version: &ImageVersion,
    ) -> Result<(StorageWriter, ContentDigest), ImageError> {
        if !(registry.is_https() || version.is_content_digest()) {
            Err(ImageError::InsecureManifest)
        } else {
            let url = self.build_url(registry, repository, "manifests", version)?;
            let request = self
                .req
                .get(url)
                .header(header::ACCEPT, media_types::MANIFEST);
            self.download(registry, request).await
        }
    }

    async fn download_blob(
        &mut self,
        registry: &Registry,
        repository: &Repository,
        content_digest: &ContentDigest,
        content_type: &HeaderValue,
    ) -> Result<StorageWriter, ImageError> {
        let url = self.build_url(registry, repository, "blobs", content_digest)?;
        let request = self.req.get(url).header(header::ACCEPT, content_type);
        let (mut writer, found_digest) = self.download(registry, request).await?;
        if &found_digest == content_digest {
            Ok(writer)
        } else {
            writer.remove_temp().await?;
            Err(ImageError::ContentDigestMismatch {
                expected: content_digest.clone(),
                found: found_digest,
            })
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
                    let (mut writer, found_digest) = self
                        .download_manifest(registry, repository, version)
                        .await?;
                    let specific_image = match image.with_found_digest(&found_digest) {
                        Ok(specific_image) => {
                            self.storage.commit_write(writer, &key).await?;
                            specific_image
                        }
                        Err(err) => {
                            writer.remove_temp().await?;
                            return Err(err);
                        }
                    };

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

    fn check_mmap_for_link(link: &Link, mmap: Mmap) -> Result<Mmap, ImageError> {
        log::trace!("{:?} mapped {} bytes", link, mmap.len());
        if mmap.len() as u64 == link.size {
            Ok(mmap)
        } else {
            Err(ImageError::UnexpectedContentSize)
        }
    }

    fn content_type_for_link(link: &Link) -> Result<HeaderValue, ImageError> {
        Ok(HeaderValue::from_str(&link.media_type)
            .map_err(|_| ImageError::InvalidContentType(link.media_type.clone()))?)
    }

    async fn pull_blob(&mut self, image: &ImageName, link: &Link) -> Result<Mmap, ImageError> {
        let (registry, repository) = self.default_registry.resolve_image_name(image);
        let key = StorageKey::Blob(ContentDigest::parse(&link.digest)?);
        let content_type = Client::content_type_for_link(link)?;
        Client::check_mmap_for_link(
            link,
            match self.storage.mmap(&key).await? {
                Some(map) => {
                    log::debug!("{} blob {} is already cached", image, link.digest);
                    map
                }
                None => match &key {
                    StorageKey::Blob(content_digest) => {
                        let writer = self
                            .download_blob(&registry, &repository, content_digest, &content_type)
                            .await?;
                        self.storage.commit_write(writer, &key).await?;
                        match self.storage.mmap(&key).await? {
                            Some(map) => map,
                            None => return Err(ImageError::StorageMissingAfterInsert),
                        }
                    }
                    _ => unreachable!(),
                },
            },
        )
    }

    async fn pull_blob_uncached(
        &mut self,
        image: &ImageName,
        link: &Link,
    ) -> Result<Mmap, ImageError> {
        let (registry, repository) = self.default_registry.resolve_image_name(image);
        let key = StorageKey::Blob(ContentDigest::parse(&link.digest)?);
        let content_type = Client::content_type_for_link(link)?;
        Client::check_mmap_for_link(
            link,
            match &key {
                StorageKey::Blob(content_digest) => {
                    let mut writer = self
                        .download_blob(&registry, &repository, content_digest, &content_type)
                        .await?;
                    let result = self.storage.mmap(&writer.key).await;
                    writer.remove_temp().await?;
                    match result? {
                        Some(map) => map,
                        None => return Err(ImageError::StorageMissingAfterInsert),
                    }
                }
                _ => unreachable!(),
            },
        )
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

    async fn pull_layers(&mut self, image: &ImageName, links: &[Link]) -> Result<(), ImageError> {
        let mut tasks = FuturesUnordered::new();
        for link in links {
            let mut client = self.clone();
            let image = image.clone();
            let link = link.clone();
            tasks.push(task::spawn(async move {
                client.pull_layer(&image, &link).await
            }));
        }
        while let Some(result) = tasks.next().await {
            result??;
        }
        Ok(())
    }

    async fn pull_layer(&mut self, image: &ImageName, link: &Link) -> Result<(), ImageError> {
        if link.media_type == media_types::LAYER_TAR_GZIP {
            self.pull_gzip_layer(image, link).await
        } else {
            Err(ImageError::UnsupportedLayerType(link.media_type.clone()))
        }
    }

    async fn pull_gzip_layer(&mut self, image: &ImageName, link: &Link) -> Result<(), ImageError> {
        let source = self.pull_blob_uncached(image, link).await?;
        let mut writer = self.storage.begin_write().await?;
        let (mut writer, result) = task::spawn_blocking(move || {
            let mut decoder = flate2::bufread::GzDecoder::new(&*source);
            let mut buffer = [0u8; 128 * 1024];
            let handle = tokio::runtime::Handle::current();
            log::info!(
                "decompressing {} bytes, header={:?}",
                source.len(),
                decoder.header()
            );
            loop {
                match decoder.read(&mut buffer) {
                    Err(err) => return (writer, Err(err)),
                    Ok(size) if size == 0 => return (writer, Ok(())),
                    Ok(size) => {
                        log::trace!("decompressed {} bytes", size);
                        match handle.block_on(async { writer.write_all(&buffer[..size]).await }) {
                            Err(err) => return (writer, Err(err)),
                            Ok(()) => (),
                        }
                    }
                }
            }
        })
        .await?;
        let result = match result {
            Err(err) => Err(err),
            Ok(()) => {
                writer.shutdown().await?;
                Ok(())
            }
        };
        match result {
            Err(err) => {
                writer.remove_temp().await?;
                Err(err.into())
            }
            Ok(()) => {
                let content_digest = writer.finalize().await?;
                let key = StorageKey::Blob(content_digest);
                self.storage.commit_write(writer, &key).await?;
                Ok(())
            }
        }
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
        let decompressed_layers = match self.check_local_rootfs_layers(&config).await? {
            Some(layers) => layers,
            None => {
                self.pull_layers(image, &manifest.layers).await?;
                self.check_local_rootfs_layers(&config)
                    .await?
                    .ok_or(ImageError::UnexpectedDecompressedLayerContent)?
            }
        };

        let mut filesystem = vfs::Filesystem::new();
        let storage = self.storage.clone();

        for layer in &decompressed_layers {
            tar::extract(&mut filesystem, &storage, layer).await?;
        }

        Ok(Arc::new(Image {
            name: specific_image,
            config,
            filesystem,
            storage,
        }))
    }

    async fn check_local_rootfs_layers(
        &mut self,
        config: &RuntimeConfig,
    ) -> Result<Option<Vec<StorageKey>>, ImageError> {
        if &config.rootfs.fs_type != FS_TYPE {
            Err(ImageError::UnsupportedRootFilesystemType(
                config.rootfs.fs_type.clone(),
            ))
        } else {
            let layer_ids = &config.rootfs.diff_ids;
            let mut layers = Vec::with_capacity(layer_ids.len());
            for digest_str in layer_ids {
                layers.push(StorageKey::Blob(ContentDigest::parse(digest_str)?));
            }
            if self.storage.all_exists(layers.iter()).await {
                Ok(Some(layers))
            } else {
                Ok(None)
            }
        }
    }
}
