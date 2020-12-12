//! Support for downloading container images from a registry server

use crate::{
    errors::ImageError,
    filesystem::storage::FileStorage,
    image::Registry,
    registry::{auth::Auth, DefaultRegistry, RegistryClient},
};

use reqwest::{
    header::{HeaderMap, HeaderValue},
    Certificate, Client, ClientBuilder,
};
use std::{
    collections::HashSet,
    convert::TryInto,
    path::{Path, PathBuf},
    time::Duration,
};

/// Builder for configuring custom [RegistryClient] instances
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
        log::debug!("using cache directory {:?}", cache_dir);
        Ok(RegistryClient::from_parts(
            FileStorage::new(cache_dir),
            self.auth,
            match self.network {
                Some(n) => Some(n.build()?),
                None => None,
            },
            self.default_registry
                .unwrap_or_else(RegistryClient::default_registry),
            self.allowed_registries,
            self.allow_http_registries,
        ))
    }
}
