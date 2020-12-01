//! Support for downloading container images from a registry server

use crate::{
    errors::ImageError,
    filesystem::{
        storage,
        storage::{FileStorage, StorageKey, StorageWriter},
        tar,
        vfs::Filesystem,
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
    Certificate, Client, ClientBuilder, RequestBuilder, Response, Url,
};
use std::{
    collections::HashSet,
    convert::TryInto,
    env,
    fmt::Display,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::task;

/// Builder for configuring custom [Client] instances
pub struct RegistryClientBuilder {
    auth: Auth,
    cache_dir: Option<PathBuf>,
    network: Option<ClientBuilder>,
    default_registry: Option<DefaultRegistry>,
    allowed_registries: Option<HashSet<Registry>>,
    allow_http_registries: bool,
}

impl RegistryClientBuilder {
    /// Start constructing a custom registry client
    pub fn new() -> Self {
        RegistryClientBuilder {
            network: Some(Client::builder().user_agent(RegistryClient::default_user_agent())),
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

    /// Only use images already in the local cache
    pub fn offline(mut self) -> Self {
        self.network = None;
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
    /// with [RegistryClient::default_cache_dir()]
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
        if let Some(network) = self.network.take() {
            self.network = Some(network.timeout(timeout));
        }
        self
    }

    /// Set a timeout for only the initial connect phase of each network request
    ///
    /// By default there is no timeout beyond those built into the networking
    /// stack.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        if let Some(network) = self.network.take() {
            self.network = Some(network.connect_timeout(timeout));
        }
        self
    }

    /// Sets the `User-Agent` header used by this client
    ///
    /// By default, the value returened by
    /// [RegistryClient::default_user_agent()] is used, which identifies the
    /// version of `bandsocks` acting as a client.
    pub fn user_agent<V>(mut self, value: V) -> Self
    where
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        if let Some(network) = self.network.take() {
            self.network = Some(network.user_agent(value));
        }
        self
    }

    /// Bind to a specific local IP address
    pub fn local_address<T>(mut self, addr: T) -> Self
    where
        T: Into<Option<std::net::IpAddr>>,
    {
        if let Some(network) = self.network.take() {
            self.network = Some(network.local_address(addr));
        }
        self
    }

    /// Set the default headers for every HTTP request
    pub fn default_request_headers(mut self, headers: HeaderMap) -> Self {
        if let Some(network) = self.network.take() {
            self.network = Some(network.default_headers(headers));
        }
        self
    }

    /// Trust an additional root certificate
    pub fn add_root_certificate(mut self, certificate: Certificate) -> Self {
        if let Some(network) = self.network.take() {
            self.network = Some(network.add_root_certificate(certificate));
        }
        self
    }

    /// Change the default registry server
    ///
    /// This registry is used for pulling images that do not specify a server.
    /// The default value if unset can be determined with
    /// [RegistryClient::default_registry()]
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

    /// Construct a RegistryClient using the parameters from this Builder
    pub fn build(self) -> Result<RegistryClient, ImageError> {
        let cache_dir = match self.cache_dir {
            Some(dir) => dir,
            None => RegistryClient::default_cache_dir()?,
        };
        log::info!("using cache directory {:?}", cache_dir);
        Ok(RegistryClient {
            storage: FileStorage::new(cache_dir),
            allowed_registries: self.allowed_registries,
            allow_http_registries: self.allow_http_registries,
            default_registry: self
                .default_registry
                .unwrap_or_else(RegistryClient::default_registry),
            auth: self.auth,
            network: match self.network {
                Some(n) => Some(n.build()?),
                None => None,
            },
        })
    }
}

/// Registry clients can download and store data from an image registry
///
/// Each client includes settings like authentication, default server, and a
/// cache storage location. One client can be used to download multiple images
/// from multiple registries.
#[derive(Clone)]
pub struct RegistryClient {
    storage: FileStorage,
    auth: Auth,
    network: Option<Client>,
    default_registry: DefaultRegistry,
    allowed_registries: Option<HashSet<Registry>>,
    allow_http_registries: bool,
}

impl RegistryClient {
    /// Construct a new registry client with default options
    pub fn new() -> Result<RegistryClient, ImageError> {
        RegistryClient::builder().build()
    }

    /// Construct a registry client with custom options, via ClientBuilder
    pub fn builder() -> RegistryClientBuilder {
        RegistryClientBuilder::new()
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
    /// [ImageName] or [RegistryClientBuilder].
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

    fn begin_get<'a, T>(
        &'a mut self,
        registry: &Registry,
        repository: &Repository,
        bucket: &'static str,
        object: T,
    ) -> Result<(&'a Client, &'a mut Auth, RequestBuilder), ImageError>
    where
        T: Display,
    {
        self.verify_registry_allowed(registry)?;

        let network = self
            .network
            .as_ref()
            .ok_or(ImageError::DownloadInOfflineMode)?;

        let url: Url = format!(
            "{}://{}/v2/{}/{}/{}",
            registry.protocol_str(),
            registry,
            repository,
            bucket,
            object
        )
        .parse()
        .expect("url components already validated");

        let req = network.get(url);
        Ok((network, &mut self.auth, req))
    }

    async fn download_response(
        &mut self,
        response: Response,
    ) -> Result<(StorageWriter, ContentDigest), ImageError> {
        log::info!("downloading {}", response.url());
        let mut response = response.error_for_status()?;
        let storage = self.storage.clone();

        // Send blocks from the async reactor to a sync thread pool for hashing
        let (send_channel, recv_channel) = std::sync::mpsc::channel::<bytes::Bytes>();
        let send_task = task::spawn(async move {
            loop {
                match response.chunk().await? {
                    Some(chunk) => send_channel.send(chunk)?,
                    None => return Ok::<(), ImageError>(()),
                }
            }
        });
        let recv_task = task::spawn_blocking(move || {
            let mut writer = storage.begin_write()?;
            while let Ok(chunk) = recv_channel.recv() {
                if let Err(err) = writer.write_all(&chunk) {
                    return Ok::<(StorageWriter, Result<ContentDigest, ImageError>), ImageError>((
                        writer,
                        Err(err.into()),
                    ));
                }
            }
            match writer.finalize() {
                Ok(content_digest) => Ok((writer, Ok(content_digest))),
                Err(err) => Ok((writer, Err(err.into()))),
            }
        });

        match tokio::join!(send_task, recv_task) {
            (Ok(Ok(())), Ok(Ok((writer, Ok(content_digest))))) => {
                log::debug!("download has digest {}", content_digest);
                Ok((writer, content_digest))
            }
            (send_result, recv_result) => {
                let (mut writer, recv_result) = recv_result??;
                task::spawn_blocking(move || writer.remove_temp()).await??;
                recv_result?;
                send_result??;
                unreachable!();
            }
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
            let (network, auth, request) =
                self.begin_get(registry, repository, "manifests", version)?;
            let response = auth
                .request(
                    registry,
                    network,
                    request.header(header::ACCEPT, media_types::MANIFEST),
                )
                .await?;
            self.download_response(response).await
        }
    }

    async fn download_blob(
        &mut self,
        registry: &Registry,
        repository: &Repository,
        content_digest: &ContentDigest,
        content_type: &HeaderValue,
    ) -> Result<StorageWriter, ImageError> {
        let (network, auth, request) =
            self.begin_get(registry, repository, "blobs", content_digest)?;
        let response = auth
            .request(
                registry,
                network,
                request.header(header::ACCEPT, content_type),
            )
            .await?;
        let (mut writer, found_digest) = self.download_response(response).await?;
        if &found_digest == content_digest {
            Ok(writer)
        } else {
            task::spawn_blocking(move || writer.remove_temp()).await??;
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
        let (specific_image, map) = match self.storage.mmap(&key)? {
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

                    let task_storage = self.storage.clone();
                    let task_image = image.clone();
                    let task_key = key.clone();
                    let specific_image = task::spawn_blocking(move || {
                        match task_image.with_found_digest(&found_digest) {
                            Ok(specific_image) => {
                                task_storage.commit_write(writer, &task_key)?;
                                Ok(specific_image)
                            }
                            Err(err) => {
                                writer.remove_temp()?;
                                Err(err)
                            }
                        }
                    })
                    .await??;

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

                    let map = match self.storage.mmap(&key)? {
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
        let content_type = RegistryClient::content_type_for_link(link)?;
        RegistryClient::check_mmap_for_link(
            link,
            match self.storage.mmap(&key)? {
                Some(map) => {
                    log::debug!("{} blob {} is already cached", image, link.digest);
                    map
                }
                None => match &key {
                    StorageKey::Blob(content_digest) => {
                        let writer = self
                            .download_blob(&registry, &repository, content_digest, &content_type)
                            .await?;

                        let task_storage = self.storage.clone();
                        match task::spawn_blocking(move || {
                            task_storage.commit_write(writer, &key)?;
                            task_storage.mmap(&key)
                        })
                        .await??
                        {
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
        let content_type = RegistryClient::content_type_for_link(link)?;
        RegistryClient::check_mmap_for_link(
            link,
            match &key {
                StorageKey::Blob(content_digest) => {
                    let mut writer = self
                        .download_blob(&registry, &repository, content_digest, &content_type)
                        .await?;

                    let task_storage = self.storage.clone();
                    match task::spawn_blocking(move || {
                        let result = task_storage.mmap(&writer.key);
                        writer.remove_temp()?;
                        result
                    })
                    .await??
                    {
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
        let task_storage = self.storage.clone();
        task::spawn_blocking(move || {
            let mut writer = task_storage.begin_write()?;
            let mut decoder = flate2::bufread::GzDecoder::new(&*source);
            let mut buffer = [0u8; 64 * 1024];
            log::info!("decompressing {} bytes", source.len());
            let result = loop {
                match decoder.read(&mut buffer) {
                    Err(err) => break Err(err),
                    Ok(size) if size == 0 => break Ok(()),
                    Ok(size) => match writer.write_all(&buffer[..size]) {
                        Err(err) => break Err(err),
                        Ok(()) => (),
                    },
                }
            };
            match result {
                Err(err) => {
                    writer.remove_temp()?;
                    Err(err.into())
                }
                Ok(()) => {
                    let content_digest = writer.finalize()?;
                    let key = StorageKey::Blob(content_digest);
                    task_storage.commit_write(writer, &key)?;
                    Ok(())
                }
            }
        })
        .await?
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

        let storage = self.storage.clone();
        let task_storage = self.storage.clone();
        let filesystem = task::spawn_blocking(move || -> Result<Filesystem, ImageError> {
            let mut filesystem = Filesystem::new();
            for layer in &decompressed_layers {
                tar::extract(&mut filesystem, &task_storage, layer)?;
            }
            Ok(filesystem)
        })
        .await??;

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
            if layers.iter().all(|layer| self.storage.exists(layer)) {
                Ok(Some(layers))
            } else {
                Ok(None)
            }
        }
    }
}
