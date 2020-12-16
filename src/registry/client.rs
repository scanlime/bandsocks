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
    registry::{auth::Auth, progress::*, DefaultRegistry, RegistryClientBuilder},
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use memmap::Mmap;
use reqwest::{header, header::HeaderValue, Client, RequestBuilder, Response, Url};
use std::{
    collections::HashSet,
    env,
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
};
use tokio::{sync::mpsc, task};

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

    pub(crate) fn from_parts(
        storage: FileStorage,
        auth: Auth,
        network: Option<Client>,
        default_registry: DefaultRegistry,
        allowed_registries: Option<HashSet<Registry>>,
        allow_http_registries: bool,
    ) -> Self {
        RegistryClient {
            storage,
            auth,
            network,
            default_registry,
            allowed_registries,
            allow_http_registries,
        }
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
        progress: &mut mpsc::Sender<PullProgress>,
        progress_resource: &Arc<ProgressResource>,
        response: Response,
    ) -> Result<(StorageWriter, ContentDigest), ImageError> {
        log::info!("downloading {}", response.url());
        let mut response = response.error_for_status()?;
        let storage = self.storage.clone();
        let mut progress = progress.clone();
        let progress_resource = progress_resource.clone();

        // Send blocks from the async reactor to a sync thread pool for hashing
        let (send_channel, recv_channel) = std::sync::mpsc::channel::<bytes::Bytes>();
        let send_task = task::spawn(async move {
            progress
                .send(PullProgress::Update(ProgressUpdate {
                    resource: progress_resource.clone(),
                    phase: ProgressPhase::Download,
                    event: match response.content_length() {
                        None => ProgressEvent::Begin,
                        Some(size) => ProgressEvent::BeginSized(size),
                    },
                }))
                .await
                .map_err(|_| ImageError::PullTaskError)?;

            let mut progress_counter = 0;
            loop {
                match response.chunk().await? {
                    Some(chunk) => {
                        progress_counter += chunk.len() as u64;
                        send_channel.send(chunk)?;
                        progress
                            .send(PullProgress::Update(ProgressUpdate {
                                resource: progress_resource.clone(),
                                phase: ProgressPhase::Download,
                                event: ProgressEvent::Progress(progress_counter),
                            }))
                            .await
                            .map_err(|_| ImageError::PullTaskError)?;
                    }
                    None => {
                        progress
                            .send(PullProgress::Update(ProgressUpdate {
                                resource: progress_resource.clone(),
                                phase: ProgressPhase::Download,
                                event: ProgressEvent::Complete,
                            }))
                            .await
                            .map_err(|_| ImageError::PullTaskError)?;

                        return Ok::<(), ImageError>(());
                    }
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
        progress: &mut mpsc::Sender<PullProgress>,
        registry: &Registry,
        repository: &Repository,
        version: &ImageVersion,
    ) -> Result<(StorageWriter, ContentDigest), ImageError> {
        if !(registry.is_https() || version.is_content_digest()) {
            Err(ImageError::InsecureManifest)
        } else {
            let progress_resource = Arc::new(ProgressResource::Manifest(
                registry.clone(),
                repository.clone(),
                version.clone(),
            ));

            progress
                .send(PullProgress::Update(ProgressUpdate {
                    resource: progress_resource.clone(),
                    phase: ProgressPhase::Connect,
                    event: ProgressEvent::Begin,
                }))
                .await
                .map_err(|_| ImageError::PullTaskError)?;

            let (network, auth, request) =
                self.begin_get(registry, repository, "manifests", version)?;
            let response = auth
                .request(
                    registry,
                    network,
                    request.header(header::ACCEPT, media_types::MANIFEST),
                )
                .await;

            progress
                .send(PullProgress::Update(ProgressUpdate {
                    resource: progress_resource.clone(),
                    phase: ProgressPhase::Connect,
                    event: ProgressEvent::Complete,
                }))
                .await
                .map_err(|_| ImageError::PullTaskError)?;

            self.download_response(progress, &progress_resource, response?)
                .await
        }
    }

    async fn download_blob(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        progress_resource: &Arc<ProgressResource>,
        registry: &Registry,
        repository: &Repository,
        content_digest: &ContentDigest,
        content_type: &HeaderValue,
    ) -> Result<StorageWriter, ImageError> {
        progress
            .send(PullProgress::Update(ProgressUpdate {
                resource: progress_resource.clone(),
                phase: ProgressPhase::Connect,
                event: ProgressEvent::Begin,
            }))
            .await
            .map_err(|_| ImageError::PullTaskError)?;

        let (network, auth, request) =
            self.begin_get(registry, repository, "blobs", content_digest)?;
        let response = auth
            .request(
                registry,
                network,
                request.header(header::ACCEPT, content_type),
            )
            .await?;

        progress
            .send(PullProgress::Update(ProgressUpdate {
                resource: progress_resource.clone(),
                phase: ProgressPhase::Connect,
                event: ProgressEvent::Complete,
            }))
            .await
            .map_err(|_| ImageError::PullTaskError)?;

        let (mut writer, found_digest) = self
            .download_response(progress, &progress_resource, response)
            .await?;
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
        progress: &mut mpsc::Sender<PullProgress>,
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
                        .download_manifest(progress, registry, repository, version)
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

    async fn pull_blob(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        image: &ImageName,
        link: &Link,
    ) -> Result<(Mmap, Arc<ProgressResource>), ImageError> {
        let (registry, repository) = self.default_registry.resolve_image_name(image);
        let content_digest = ContentDigest::parse(&link.digest)?;
        let key = StorageKey::Blob(content_digest.clone());
        let progress_resource = Arc::new(ProgressResource::Blob(content_digest.clone()));
        let content_type = RegistryClient::content_type_for_link(link)?;
        let mmap = RegistryClient::check_mmap_for_link(
            link,
            match self.storage.mmap(&key)? {
                Some(map) => {
                    log::debug!("{} blob {} is already cached", image, link.digest);
                    map
                }
                None => match &key {
                    StorageKey::Blob(content_digest) => {
                        let writer = self
                            .download_blob(
                                progress,
                                &progress_resource,
                                &registry,
                                &repository,
                                content_digest,
                                &content_type,
                            )
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
        )?;
        Ok((mmap, progress_resource))
    }

    async fn pull_blob_uncached(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        image: &ImageName,
        link: &Link,
    ) -> Result<(Mmap, Arc<ProgressResource>), ImageError> {
        let (registry, repository) = self.default_registry.resolve_image_name(image);
        let content_type = RegistryClient::content_type_for_link(link)?;
        let content_digest = ContentDigest::parse(&link.digest)?;
        let key = StorageKey::Blob(content_digest.clone());
        let progress_resource = Arc::new(ProgressResource::Blob(content_digest.clone()));
        let mmap = RegistryClient::check_mmap_for_link(
            link,
            match &key {
                StorageKey::Blob(content_digest) => {
                    let mut writer = self
                        .download_blob(
                            progress,
                            &progress_resource,
                            &registry,
                            &repository,
                            content_digest,
                            &content_type,
                        )
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
        )?;
        Ok((mmap, progress_resource))
    }

    async fn pull_runtime_config(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        image: &ImageName,
        link: &Link,
    ) -> Result<RuntimeConfig, ImageError> {
        if link.media_type == media_types::RUNTIME_CONFIG {
            let (mapref, _progress_resource) = self.pull_blob(progress, image, link).await?;
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

    async fn pull_layers(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        image: &ImageName,
        links: &[Link],
    ) -> Result<(), ImageError> {
        let mut tasks = FuturesUnordered::new();
        for link in links {
            let mut client = self.clone();
            let mut progress = progress.clone();
            let image = image.clone();
            let link = link.clone();
            tasks.push(task::spawn(async move {
                client.pull_layer(&mut progress, &image, &link).await
            }));
        }
        while let Some(result) = tasks.next().await {
            result??;
        }
        Ok(())
    }

    async fn pull_layer(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        image: &ImageName,
        link: &Link,
    ) -> Result<(), ImageError> {
        if link.media_type == media_types::LAYER_TAR_GZIP {
            self.pull_gzip_layer(progress, image, link).await
        } else {
            Err(ImageError::UnsupportedLayerType(link.media_type.clone()))
        }
    }

    async fn pull_gzip_layer(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        image: &ImageName,
        link: &Link,
    ) -> Result<(), ImageError> {
        let (source, progress_resource) = self.pull_blob_uncached(progress, image, link).await?;
        let task_storage = self.storage.clone();
        let mut task_progress = progress.clone();
        let task_progress_resource = progress_resource.clone();

        progress
            .send(PullProgress::Update(ProgressUpdate {
                resource: progress_resource.clone(),
                phase: ProgressPhase::Decompress,
                event: ProgressEvent::BeginSized(source.len() as u64),
            }))
            .await
            .map_err(|_| ImageError::PullTaskError)?;

        task::spawn_blocking(move || -> Result<(), ImageError> {
            let mut writer = task_storage.begin_write()?;
            let mut decoder = flate2::bufread::GzDecoder::new(std::io::Cursor::new(&*source));
            let mut buffer = [0u8; 64 * 1024];
            log::info!("decompressing {} bytes", source.len());

            let result: std::io::Result<()> = loop {
                match decoder.read(&mut buffer) {
                    Err(err) => break Err(err),
                    Ok(size) if size == 0 => break Ok(()),
                    Ok(size) => match writer.write_all(&buffer[..size]) {
                        Err(err) => break Err(err),
                        Ok(()) => {
                            let _ = task_progress.try_send(PullProgress::Update(ProgressUpdate {
                                resource: task_progress_resource.clone(),
                                phase: ProgressPhase::Decompress,
                                event: ProgressEvent::Progress(decoder.get_ref().position()),
                            }));
                        }
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
        .await??;

        progress
            .send(PullProgress::Update(ProgressUpdate {
                resource: progress_resource.clone(),
                phase: ProgressPhase::Decompress,
                event: ProgressEvent::Complete,
            }))
            .await
            .map_err(|_| ImageError::PullTaskError)?;
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
    ///
    /// This is equivalent to [RegistryClient::pull_progress()] followed by
    /// [Pull::wait()].
    pub async fn pull(&self, image: &ImageName) -> Result<Arc<Image>, ImageError> {
        self.pull_progress(image).wait().await
    }

    /// Start to pull an image, and return progress updates
    pub fn pull_progress(&self, image: &ImageName) -> Pull {
        let (mut sender, receiver) = mpsc::channel(128);
        let image = image.clone();
        let mut client = self.clone();
        let _ = task::spawn(async move {
            let result = client.pull_with_progress_channel(&mut sender, &image).await;
            let _ = sender.send(PullProgress::Done(result)).await;
        });
        Pull { receiver }
    }

    async fn pull_with_progress_channel(
        &mut self,
        progress: &mut mpsc::Sender<PullProgress>,
        image: &ImageName,
    ) -> Result<Arc<Image>, ImageError> {
        let (specific_image, manifest) = self.pull_manifest(progress, image).await?;
        let config = self
            .pull_runtime_config(progress, image, &manifest.config)
            .await?;
        let decompressed_layers = match self.check_local_rootfs_layers(&config).await? {
            Some(layers) => layers,
            None => {
                self.pull_layers(progress, image, &manifest.layers).await?;
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
