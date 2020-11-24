//! Container images and image identity

#[cfg(test)] mod tests;

use crate::{
    errors::ImageError,
    filesystem::{storage::FileStorage, vfs::Filesystem},
    manifest::RuntimeConfig,
};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
    hash::{Hash, Hasher},
    io::Write,
    ops::Range,
    str,
    str::FromStr,
};

/// Loaded data for a container image
///
/// This is the actual configuration and filesystem data associated with a
/// container image. It is immutable, and multiple running containers can use
/// one image.
///
/// The virtual filesystem stores all metadata in memory, but file contents are
/// referenced as needed from the configured disk cache.
#[derive(Debug)]
pub struct Image {
    pub(crate) name: ImageName,
    pub(crate) config: RuntimeConfig,
    pub(crate) filesystem: Filesystem,
    pub(crate) storage: FileStorage,
}

impl Image {
    /// Get the digest identifying this image's content and configuration
    pub fn content_digest(&self) -> ContentDigest {
        self.name()
            .content_digest()
            .expect("loaded images must always have a digest")
    }

    /// Get the name of this image, including its content digest
    pub fn name(&self) -> &ImageName {
        &self.name
    }
}

/// Parsed Docker-style image reference
///
/// This is an owned struct representing a docker "reference" (like a URI) which
/// refers to an image, optionally at a specific version, which can be fetched
/// from a registry server (possibly the configured default).
///
/// This tries to be format-compatible with Docker including its quirks.
///
/// A complete image name contains a [Registry], [Repository], [Tag], and
/// [ContentDigest] in that order. Only the [Repository] component is mandatory.
///
/// The [Tag] always begins with a `:` and the [ContentDigest] with an `@`, but
/// delineating the optional [Registry] and the first section of the
/// [Repository] requires heuristics. If this first section includes any dot (.)
/// or colon (:) characters it is assumed to be a repository server. This same
/// property (see [Registry]) ensures that the parsed registry uses https. The
/// additional exception is a special case for "localhost", which is always
/// interpreted as a registry name. Additionally, because it has no dots, it is
/// interpreted as an unencrypted http registry at localhost.
///
/// When a [ContentDigest] is specified, it securely identifies the specific
/// contents of an image's layer data and manifest. Remember that a name without
/// a digest is only as trustworthy as the registry server and our connection to
/// it.
#[derive(Clone)]
pub struct ImageName {
    serialized: String,
    registry_pos: Option<Range<usize>>,
    repository_pos: Range<usize>,
    tag_pos: Option<Range<usize>>,
    digest_pos: Option<Range<usize>>,
}

impl ImageName {
    /// Returns a reference to the existing string representation of an
    /// [ImageName]
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Parse an [ImageName] from its component pieces
    ///
    /// This may fail either because of a problem with one of the components,
    /// or because the resulting path would be parsed in a manner other than
    /// intended. For example, a registry name could be parsed as the first
    /// section of the repository path.
    pub fn from_parts(
        registry: Option<&str>,
        repository: &str,
        tag: Option<&str>,
        digest: Option<&str>,
    ) -> Result<Self, ImageError> {
        let mut buffer = Vec::new();
        if let Some(registry) = registry {
            write!(&mut buffer, "{}/", registry)?;
        }
        write!(&mut buffer, "{}", repository)?;
        if let Some(tag) = tag {
            write!(&mut buffer, ":{}", tag)?;
        }
        if let Some(digest) = digest {
            write!(&mut buffer, "@{}", digest)?;
        }
        let combined = str::from_utf8(&buffer).unwrap();
        let parsed = ImageName::parse(combined)?;
        if parsed.registry_str().as_deref() == registry
            && parsed.repository_str() == repository
            && parsed.tag_str().as_deref() == tag
            && parsed.content_digest_str().as_deref() == digest
        {
            Ok(parsed)
        } else {
            // Parsing ambiguity
            Err(ImageError::InvalidReferenceFormat(combined.to_owned()))
        }
    }

    /// Return references to the parsed components within this [ImageName]
    pub fn as_parts(&self) -> (Option<&str>, &str, Option<&str>, Option<&str>) {
        (
            self.registry_str(),
            self.repository_str(),
            self.tag_str(),
            self.content_digest_str(),
        )
    }

    /// Returns the most specific available version
    ///
    /// If the image name includes a digest, this returns the digest. Otherwise,
    /// it returns the tag, defaulting to `latest` if no tag is set.
    pub fn version(&self) -> ImageVersion {
        if self.content_digest_str().is_some() {
            return ImageVersion::ContentDigest(self.content_digest().unwrap());
        }
        if self.tag_str().is_some() {
            return ImageVersion::Tag(self.tag().unwrap());
        }
        ImageVersion::Tag(Tag::latest())
    }

    /// Parse a [prim@str] as an [ImageName]
    pub fn parse(s: &str) -> Result<Self, ImageError> {
        lazy_static! {
            static ref HAS_REGISTRY: Regex = Regex::new(concat!(
                "^",
                "(?:", // alternatives group
                /* */ "(?:", // one option: a domain with at least one dot
                /* -- */ "(?:", // First domain component
                /* -- -- */ "[a-zA-Z0-9]|",
                /* -- -- */ "[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9]",
                /* -- */ ")",
                /* -- */ "(?:", // Additional domain components
                /* -- -- */ "\\.",
                /* -- -- */ "(?:",
                /* -- -- -- */ "[a-zA-Z0-9]|",
                /* -- -- -- */ "[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9]",
                /* -- -- */ ")",
                /* -- */ ")+",
                /* -- */ "(?::[0-9]+)?", // Optional port number
                /*  */ ")",
                /* */ "|(?:", // another option: no dots, but there's a port number
                /* -- */ "(?:", // Only domain component
                /* -- -- */ "[a-zA-Z0-9]|",
                /* -- -- */ "[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9]",
                /* -- */ ")",
                /* -- */ "(?::[0-9]+)", // port number
                /*  */ ")",
                /* */ "|(?:", // special case for localhost
                /* -- */ "localhost",
                /* -- */ "(?::[0-9]+)?", // Optional port number
                /*  */ ")",
                ")", // end of alternatives
                "/", // done matching at the first slash, which is not optional here
            )).unwrap();
            static ref WITH_REGISTRY: Regex = Regex::new(&format!(
                "^{}/{}(:{})?(@{})?$",
                Registry::regex_str(),
                Repository::regex_str(),
                Tag::regex_str(),
                ContentDigest::regex_str()
            ))
            .unwrap();
            static ref NO_REGISTRY: Regex = Regex::new(&format!(
                "^{}(:{})?(@{})?$",
                Repository::regex_str(),
                Tag::regex_str(),
                ContentDigest::regex_str()
            ))
                .unwrap();
        }
        if HAS_REGISTRY.is_match(s) {
            match WITH_REGISTRY.captures(s) {
                None => Err(ImageError::InvalidReferenceFormat(s.to_owned())),
                Some(captures) => Ok(ImageName {
                    serialized: s.to_owned(),
                    registry_pos: Some(captures.name("reg").unwrap().range()),
                    repository_pos: captures.name("repo").unwrap().range(),
                    tag_pos: captures.name("tag").map(|m| m.range()),
                    digest_pos: captures.name("dig").map(|m| m.range()),
                }),
            }
        } else {
            match NO_REGISTRY.captures(s) {
                None => Err(ImageError::InvalidReferenceFormat(s.to_owned())),
                Some(captures) => Ok(ImageName {
                    serialized: s.to_owned(),
                    registry_pos: None,
                    repository_pos: captures.name("repo").unwrap().range(),
                    tag_pos: captures.name("tag").map(|m| m.range()),
                    digest_pos: captures.name("dig").map(|m| m.range()),
                }),
            }
        }
    }

    /// Returns a reference to the optional registry portion of the string.
    pub fn registry_str(&self) -> Option<&str> {
        self.registry_pos
            .as_ref()
            .map(|pos| &self.serialized[pos.clone()])
    }

    /// Returns a reference to the repository portion of the string
    pub fn repository_str(&self) -> &str {
        &self.serialized[self.repository_pos.clone()]
    }

    /// Returns a reference to the optional tag portion of the string.
    pub fn tag_str(&self) -> Option<&str> {
        self.tag_pos
            .as_ref()
            .map(|pos| &self.serialized[pos.clone()])
    }

    /// Returns a reference to the optional digest portion of the string.
    pub fn content_digest_str(&self) -> Option<&str> {
        self.digest_pos
            .as_ref()
            .map(|pos| &self.serialized[pos.clone()])
    }

    /// Returns the registry portion as a new object
    pub fn registry(&self) -> Option<Registry> {
        self.registry_str()
            .map(|s| Registry::parse(s).expect("already parsed"))
    }

    /// Returns the repository portion as a new object
    pub fn repository(&self) -> Repository {
        Repository::parse(self.repository_str()).expect("already parsed")
    }

    /// Returns the tag portion as a new object
    pub fn tag(&self) -> Option<Tag> {
        self.tag_str()
            .map(|s| Tag::parse(s).expect("already parsed"))
    }

    /// Returns the digest portion as a new object
    pub fn content_digest(&self) -> Option<ContentDigest> {
        self.content_digest_str()
            .map(|s| ContentDigest::parse(s).expect("already parsed"))
    }

    /// Make a new ImageName based on this one, including a specific
    /// [ContentDigest]
    ///
    /// If this image already has a content digest, it is verified against the
    /// provided one and a [ImageError::ContentDigestMismatch] is returned on
    /// mismatch.
    pub fn with_specific_digest(&self, digest: &ContentDigest) -> Result<Self, ImageError> {
        match self.content_digest() {
            None => Ok(()),
            Some(matching) if &matching == digest => Ok(()),
            Some(mismatch) => Err(ImageError::ContentDigestMismatch {
                expected: digest.clone(),
                found: mismatch,
            }),
        }?;
        ImageName::from_parts(
            self.registry_str(),
            self.repository_str(),
            self.tag_str(),
            Some(digest.as_str()),
        )
    }
}

impl Eq for ImageName {}

impl PartialEq for ImageName {
    fn eq(&self, other: &Self) -> bool {
        self.serialized.eq(&other.serialized)
    }
}

impl FromStr for ImageName {
    type Err = ImageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ImageName::parse(s)
    }
}

impl fmt::Display for ImageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for ImageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Hash for ImageName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.serialized.hash(state);
    }
}

impl Ord for ImageName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.serialized.cmp(&other.serialized)
    }
}

impl PartialOrd for ImageName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.serialized.partial_cmp(&other.serialized)
    }
}

/// Name of a Docker-style image registry server
///
/// This is a domain name, with an optional port. Typically the protocol is
/// https, but we include the same heuristic Docker uses to improve the
/// ergonomics of development setups: if a domain has no dots in it, the
/// protocol switches to unencrypted http.
///
/// For information on running your own registry server for development, see <https://docs.docker.com/registry/deploying/>

#[derive(Clone)]
pub struct Registry {
    serialized: String,
    domain_pos: Range<usize>,
    port: Option<u16>,
    is_https: bool,
}

impl Registry {
    /// Returns a reference to the existing string representation of a
    /// [Registry]
    ///
    /// Always consists of a domain name with optional port, which have been
    /// validated by the parser. May include alphanumeric characters, at most
    /// one colon, and it may include single dots at positions other than the
    /// beginning of the string.
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Parse a [prim@str] as a [Registry]
    pub fn parse(s: &str) -> Result<Self, ImageError> {
        lazy_static! {
            static ref RE: Regex = Regex::new(&format!("^{}$", Registry::regex_str(),)).unwrap();
        }
        match RE.captures(s) {
            None => Err(ImageError::InvalidReferenceFormat(s.to_owned())),
            Some(captures) => {
                let domain = captures.name("reg_d").unwrap();
                Ok(Registry {
                    serialized: s.to_owned(),
                    domain_pos: domain.range(),
                    is_https: domain.as_str().contains('.'),
                    port: captures.name("reg_p").map(|m| m.as_str().parse().unwrap()),
                })
            }
        }
    }

    /// Returns a reference to the domain portion of the string
    pub fn domain_str(&self) -> &str {
        &self.serialized[self.domain_pos.clone()]
    }

    /// Returns the port, if present
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// Are we using https to connect to the registry?
    pub fn is_https(&self) -> bool {
        self.is_https
    }

    /// The protocol to use, either "http" or "https"
    pub fn protocol_str(&self) -> &str {
        if self.is_https() {
            "https"
        } else {
            "http"
        }
    }

    fn regex_str() -> &'static str {
        concat!(
            "(?P<reg>", // Main registry match group
            /*  */ "(?P<reg_d>", // registry domain match group
            /* -- */ "(?:", // First domain component
            /* -- -- */ "[a-zA-Z0-9]|",
            /* -- -- */ "[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9]",
            /* -- */ ")",
            /* -- */ "(?:", // Optional additional domain components
            /* -- -- */ "\\.",
            /* -- -- */ "(?:",
            /* -- -- -- */ "[a-zA-Z0-9]|",
            /* -- -- -- */ "[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9]",
            /* -- -- */ ")",
            /* -- */ ")*",
            /*  */ ")", // end registry domain match group
            /*  */ "(?:", // Optional port number
            /* -- */ "[:]",
            /* -- */ "(?P<reg_p>", // Registry port group
            /* -- -- */ "[0-9]+",
            /* -- */ ")",
            /*  */ ")?",
            ")",
        )
    }
}

impl Eq for Registry {}

impl PartialEq for Registry {
    fn eq(&self, other: &Self) -> bool {
        self.serialized.eq(&other.serialized)
    }
}

impl FromStr for Registry {
    type Err = ImageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Registry::parse(s)
    }
}

impl fmt::Display for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Hash for Registry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.serialized.hash(state);
    }
}

impl Ord for Registry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.serialized.cmp(&other.serialized)
    }
}

impl PartialOrd for Registry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.serialized.partial_cmp(&other.serialized)
    }
}

/// Name of a Docker-style image repository
///
/// A repository contains multiple versions (tags, digests) of images that can
/// be referenced under a common name. Repository names are path-like groupings
/// of lowercase alphanumeric segments separated by slashes. Each grouping may
/// also contain internal separator characters: single periods, single
/// underscores, double underscores, or any number of dashes.
#[derive(Clone)]
pub struct Repository {
    serialized: String,
}

/// Iterator over components of a Repository path
pub struct RepositoryIter<'a> {
    remaining: Option<&'a str>,
}

impl<'a> Iterator for RepositoryIter<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        self.remaining.map(|remaining| {
            let mut parts = remaining.splitn(2, '/');
            let first = parts.next().unwrap();
            let second = parts.next();
            self.remaining = second;
            first
        })
    }
}

impl Repository {
    /// Returns a reference to the existing string representation of a
    /// [Repository]
    ///
    /// Always consists of at least one path segment, separated by slashes.
    /// Characters are limited to lowercase alphanumeric, single internal
    /// forward slashes, and dots or dashes which do not begin a path
    /// segment.
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Parse a [prim@str] as a [Repository]
    ///
    /// ```
    /// # use bandsocks::image::Repository;
    /// let repo = Repository::parse("some/path").unwrap();
    /// let parts: Vec<&str> = repo.iter().collect();
    /// assert_eq!(parts, vec!["some", "path"])
    /// ```
    pub fn parse(s: &str) -> Result<Self, ImageError> {
        lazy_static! {
            static ref RE: Regex = Regex::new(&format!("^{}$", Repository::regex_str(),)).unwrap();
        }
        match RE.is_match(s) {
            false => Err(ImageError::InvalidReferenceFormat(s.to_owned())),
            true => Ok(Repository {
                serialized: s.to_owned(),
            }),
        }
    }

    /// Produce an iterator over the slash-separated parts of a repository path
    pub fn iter(&self) -> RepositoryIter {
        RepositoryIter {
            remaining: Some(&self.serialized),
        }
    }

    /// Join this path to another with a slash, forming a new repository path
    ///
    /// Note that it's never legal for a repository path to begin or end
    /// with a slash, or for any component to start with a dot.
    pub fn join(&self, other: &Self) -> Self {
        Repository {
            serialized: format!("{}/{}", self.serialized, other.serialized),
        }
    }

    fn regex_str() -> &'static str {
        concat!(
            "(?P<repo>", // Repository match group
            /*  */ "(?:", // Main name component
            /* -- */ "[a-z0-9]+",
            /* -- */ "(?:",
            /* -- -- */ "(?:[._]|__|[-]*)", // allowed separators
            /* -- -- */ "[a-z0-9]+",
            /* -- */ ")*", // multiple separator groups
            /*  */ ")", // end first name component
            /*  */ "(?:", // Optional additional name components
            /* -- */ "/",
            /* -- */ "[a-z0-9]+",
            /* -- */ "(?:",
            /* -- -- */ "(?:[._]|__|[-]*)", // allowed separators
            /* -- -- */ "[a-z0-9]+",
            /* -- */ ")*", // multiple separator groups
            /*  */ ")*", // multiple additional name components
            ")"
        )
    }
}

impl Eq for Repository {}

impl PartialEq for Repository {
    fn eq(&self, other: &Self) -> bool {
        self.serialized.eq(&other.serialized)
    }
}

impl FromStr for Repository {
    type Err = ImageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Repository::parse(s)
    }
}

impl fmt::Display for Repository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for Repository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Hash for Repository {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.serialized.hash(state);
    }
}

impl Ord for Repository {
    fn cmp(&self, other: &Self) -> Ordering {
        self.serialized.cmp(&other.serialized)
    }
}

impl PartialOrd for Repository {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.serialized.partial_cmp(&other.serialized)
    }
}

/// A tag identifying a specific image version by name
///
/// Tags are up to 128 characters long, including alphanumeric characters and
/// underscores appearing anywhere in the string, and dots or dashes appearing
/// anywhere except the beginning.
#[derive(Clone)]
pub struct Tag {
    serialized: String,
}

static LATEST_STR: &str = "latest";

impl Tag {
    /// Returns a reference to the existing string representation of a [Tag]
    ///
    /// Tags are up to 128 characters long, including alphanumeric characters
    /// and underscores appearing anywhere in the string, and dots or dashes
    /// appearing anywhere except the beginning.
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Parse a [prim@str] as a [Tag]
    pub fn parse(s: &str) -> Result<Self, ImageError> {
        lazy_static! {
            static ref RE: Regex = Regex::new(&format!("^{}$", Tag::regex_str(),)).unwrap();
        }
        match RE.is_match(s) {
            false => Err(ImageError::InvalidReferenceFormat(s.to_owned())),
            true => Ok(Tag {
                serialized: s.to_owned(),
            }),
        }
    }

    /// Returns the special tag `latest`
    pub fn latest() -> Self {
        Tag {
            serialized: LATEST_STR.to_owned(),
        }
    }

    /// Is this the special tag `latest`?
    pub fn is_latest(&self) -> bool {
        self.serialized == LATEST_STR
    }

    fn regex_str() -> &'static str {
        "(?P<tag>[a-zA-Z0-9_][a-zA-Z0-9_.-]{0,127})"
    }
}

impl Eq for Tag {}

impl PartialEq for Tag {
    fn eq(&self, other: &Tag) -> bool {
        self.serialized.eq(&other.serialized)
    }
}

impl FromStr for Tag {
    type Err = ImageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Tag::parse(s)
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Hash for Tag {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.serialized.hash(state);
    }
}

impl Ord for Tag {
    fn cmp(&self, other: &Self) -> Ordering {
        self.serialized.cmp(&other.serialized)
    }
}

impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.serialized.partial_cmp(&other.serialized)
    }
}

/// A digest securely identifies the specific contents of a binary object
///
/// Digests include the hash format, which is currently always `sha256`
#[derive(Clone)]
pub struct ContentDigest {
    serialized: String,
    format_pos: Range<usize>,
    hex_pos: Range<usize>,
}

impl Eq for ContentDigest {}

impl PartialEq for ContentDigest {
    fn eq(&self, other: &Self) -> bool {
        self.serialized.eq(&other.serialized)
    }
}

impl FromStr for ContentDigest {
    type Err = ImageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ContentDigest::parse(s)
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Hash for ContentDigest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.serialized.hash(state);
    }
}

impl Ord for ContentDigest {
    fn cmp(&self, other: &Self) -> Ordering {
        self.serialized.cmp(&other.serialized)
    }
}

impl PartialOrd for ContentDigest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.serialized.partial_cmp(&other.serialized)
    }
}

impl ContentDigest {
    /// Returns a reference to the existing string representation of a
    /// [ContentDigest]
    ///
    /// This string always has a single colon. After the colon is 32 or more
    /// characters which will always be lowercase hexadecimal digits. The format
    /// specifier before this colon is alphanumeric, with plus, dash,
    /// underscore, or dot characters allowed as separators between valid
    /// groups of alphanumeric characters.
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Create a new ContentDigest from parts
    ///
    /// The format string and hex string are assembled and parsed.
    pub fn from_parts<T: fmt::LowerHex>(
        format_part: &str,
        hex_part: &T,
    ) -> Result<Self, ImageError> {
        ContentDigest::parse(&format!("{}:{:x}", format_part, hex_part))
    }

    /// Create a new ContentDigest from content data
    ///
    /// This hashes the content using the the `sha256` algorithm.
    ///
    /// ```
    /// # use bandsocks::image::ContentDigest;
    /// let digest = ContentDigest::from_content(b"cat");
    /// assert_eq!(digest.as_str(), "sha256:77af778b51abd4a3c51c5ddd97204a9c3ae614ebccb75a606c3b6865aed6744e");
    /// ```
    pub fn from_content(content_bytes: &[u8]) -> Self {
        ContentDigest::from_parts("sha256", &Sha256::digest(content_bytes)).unwrap()
    }

    /// Parse a [prim@str] as a [ContentDigest]
    ///
    /// ```
    /// # use bandsocks::image::ContentDigest;
    /// let digest = ContentDigest::parse("format:00112233445566778899aabbccddeeff").unwrap();
    /// assert_eq!(digest.format_str(), "format");
    /// assert_eq!(digest.hex_str(), "00112233445566778899aabbccddeeff")
    /// ```
    pub fn parse(s: &str) -> Result<Self, ImageError> {
        lazy_static! {
            static ref RE: Regex =
                Regex::new(&format!("^{}$", ContentDigest::regex_str(),)).unwrap();
        }
        match RE.captures(s) {
            None => Err(ImageError::InvalidReferenceFormat(s.to_owned())),
            Some(captures) => Ok(ContentDigest {
                serialized: s.to_owned(),
                format_pos: captures.name("dig_f").unwrap().range(),
                hex_pos: captures.name("dig_h").unwrap().range(),
            }),
        }
    }

    /// Return a reference to the format string portion of this digest.
    ///
    /// Currently this is `sha256` for all digests we create or recognize.
    pub fn format_str(&self) -> &str {
        &self.serialized[self.format_pos.clone()]
    }

    /// Return a reference to the hexadecimal string portion of this digest.
    ///
    /// This is guaranteed to be a string of at least 32 hex digits.
    pub fn hex_str(&self) -> &str {
        &self.serialized[self.hex_pos.clone()]
    }

    fn regex_str() -> &'static str {
        concat!(
            "(?P<dig>", // digest group
            /*  */ "(?P<dig_f>", // digest format group
            /* -- */ "(?:", // first format component
            /* -- -- */ "[a-zA-Z]",
            /* -- -- */ "[a-zA-Z0-9]*",
            /* -- */ ")",
            /* -- */ "(?:", // Additional format component
            /* -- -- */ "[-_+.]", // separators allowed in the digest format
            /* -- -- */ "[a-zA-Z]",
            /* -- -- */ "[a-zA-Z0-9]*",
            /* -- */ ")*",
            /*  */ ")", // end digest format group
            /*  */ "[:]", // Main separator
            /*  */ "(?P<dig_h>", // digest hex group
            /* -- */ "[a-f0-9]{32,}",
            /*  */ ")",
            ")",
        )
    }
}

/// Either an image tag or a content digest
///
/// An [ImageName] includes an optional tag and an optional content digest. Only
/// the most specific available version is used to actually download an image,
/// though. Any [ImageName] can be resolved into an [ImageVersion] that is
/// either a digest, a tag, or the special tag "latest".

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ImageVersion {
    Tag(Tag),
    ContentDigest(ContentDigest),
}

impl ImageVersion {
    /// Returns a reference to the existing string representation of an
    /// [ImageVersion]
    pub fn as_str(&self) -> &str {
        match self {
            ImageVersion::Tag(tag) => tag.as_str(),
            ImageVersion::ContentDigest(content_digest) => content_digest.as_str(),
        }
    }

    /// Parse a [prim@str] as an [ImageVersion]
    pub fn parse(s: &str) -> Result<Self, ImageError> {
        if s.contains(':') {
            Ok(ImageVersion::ContentDigest(ContentDigest::parse(s)?))
        } else {
            Ok(ImageVersion::Tag(Tag::parse(s)?))
        }
    }
}

impl FromStr for ImageVersion {
    type Err = ImageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ImageVersion::parse(s)
    }
}

impl fmt::Display for ImageVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for ImageVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}
