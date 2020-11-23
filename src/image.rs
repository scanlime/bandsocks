//! Container images and image identity

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
/// The filesystem stores all metadata in memory, but file contents are
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
/// This tries to be format-compatible with Docker as described by the
/// authoritative reference at <https://github.com/docker/distribution/blob/master/reference/regexp.go>
///
/// A complete image name contains a [Registry], [Repository], [Tag], and
/// [ContentDigest] in that order. Only the [Repository] component is mandatory.
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

    /// Parse a [prim@str] as an [ImageName]
    pub fn parse(s: &str) -> Result<Self, ImageError> {
        lazy_static! {
            static ref RE: Regex = Regex::new(&format!(
                concat!("^", "(?:{}/)?", "{}", "(?:[:]{})?", "(?:[@]{})?", "$",),
                Registry::regex_str(),
                Repository::regex_str(),
                Tag::regex_str(),
                ContentDigest::regex_str()
            ))
            .unwrap();
        }
        match RE.captures(s) {
            None => Err(ImageError::InvalidReferenceFormat(s.to_owned())),
            Some(captures) => Ok(ImageName {
                serialized: s.to_owned(),
                registry_pos: captures.name("reg").map(|m| m.range()),
                repository_pos: captures.name("repo").unwrap().range(),
                tag_pos: captures.name("tag").map(|m| m.range()),
                digest_pos: captures.name("dig").map(|m| m.range()),
            }),
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
        match self.content_digest_str() {
            None => Ok(()),
            Some(matching) if matching == digest.as_str() => Ok(()),
            Some(mismatch) => Err(ImageError::ContentDigestMismatch(
                mismatch.to_owned(),
                digest.as_str().to_string(),
            )),
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
    pub fn protocol_str(&self) -> &'static str {
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

impl Tag {
    /// Returns a reference to the existing string representation of a [Tag]
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
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Create a new ContentDigest from parts
    ///
    /// The format string and hex string are assembled and parsed.
    pub fn from_parts(format_part: &str, hex_part: &str) -> Result<Self, ImageError> {
        ContentDigest::parse(&format!("{}:{}", format_part, hex_part))
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
        ContentDigest::parse(&format!("sha256:{:x}", Sha256::digest(content_bytes))).unwrap()
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
            /* -- */ "[a-fA-F0-9]{32,}",
            /*  */ ")",
            ")",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_image_name() {
        assert!(ImageName::parse("balls").is_ok());
        assert!(ImageName::parse("balls/").is_err());
        assert!(ImageName::parse("balls/etc").is_ok());
        assert!(ImageName::parse("balls/etc/and/more").is_ok());
        assert!(ImageName::parse("b-a-l-l-s").is_ok());
        assert!(ImageName::parse("-balls").is_err());
        assert!(ImageName::parse("--balls").is_err());
        assert!(ImageName::parse("b--alls").is_ok());
        assert!(ImageName::parse("balls.io/image/of/my/balls").is_ok());
        assert!(ImageName::parse("balls.io/image/of/my/balls:").is_err());
        assert!(ImageName::parse("balls.io/image/of/my/balls:?").is_err());
        assert!(ImageName::parse("balls.io/image/of/my/balls:0").is_ok());
        assert!(ImageName::parse("balls.io/image/of/my/balls:.").is_err());
        assert!(ImageName::parse("balls.io/image/of/my/balls:0.0").is_ok());
        assert!(ImageName::parse("balls.io/image/of//balls").is_err());
        assert!(ImageName::parse(" balls").is_err());
        assert!(ImageName::parse("balls ").is_err());
        assert!(ImageName::parse("balls:69").is_ok());
        assert!(ImageName::parse("balls:6.9").is_ok());
        assert!(ImageName::parse("balls:").is_err());
        assert!(ImageName::parse("balls.io:69/ball").is_ok());
        assert!(ImageName::parse("balls.io:/ball").is_err());

        assert!(ImageName::parse("").is_err());
        assert!(ImageName::parse("blah ").is_err());
        assert!(ImageName::parse("blah/").is_err());
        assert!(ImageName::parse(" blah").is_err());
        assert!(ImageName::parse("/blah").is_err());

        let p = ImageName::parse("blah").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "blah".parse().unwrap());
        assert_eq!(p.tag(), None);
        assert_eq!(p.content_digest(), None);

        let p = ImageName::parse("localhost").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "localhost".parse().unwrap());
        assert_eq!(p.tag(), None);
        assert_eq!(p.content_digest(), None);

        let p = ImageName::parse("library").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "library".parse().unwrap());
        assert_eq!(p.tag(), None);
        assert_eq!(p.content_digest(), None);

        let p = ImageName::parse("foo/bar").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "foo/bar".parse().unwrap());
        assert_eq!(p.tag(), None);
        assert_eq!(p.content_digest(), None);

        let p = ImageName::parse("blah:tag").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "blah".parse().unwrap());
        assert_eq!(p.tag(), Some("tag".parse().unwrap()));
        assert_eq!(p.content_digest(), None);

        let p = ImageName::parse("blah@fm:00112233445566778899aabbccddeeff").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "blah".parse().unwrap());
        assert_eq!(p.tag(), None);
        assert_eq!(
            p.content_digest(),
            Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
        );

        let p = ImageName::parse("blah:tag@fm:00112233445566778899aabbccddeeff").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "blah".parse().unwrap());
        assert_eq!(p.tag(), Some("tag".parse().unwrap()));
        assert_eq!(
            p.content_digest(),
            Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
        );

        let p = ImageName::parse("floop/blah:tag@fm:00112233445566778899aabbccddeeff").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "floop/blah".parse().unwrap());
        assert_eq!(p.tag(), Some("tag".parse().unwrap()));
        assert_eq!(
            p.content_digest(),
            Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
        );

        let p = ImageName::parse("oop/boop/blah:tag@fm:00112233445566778899aabbccddeeff").unwrap();
        assert_eq!(p.registry(), None);
        assert_eq!(p.repository(), "oop/boop/blah".parse().unwrap());
        assert_eq!(p.tag(), Some("tag".parse().unwrap()));
        assert_eq!(
            p.content_digest(),
            Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
        );
    }

    #[test]
    fn parse_digest_name() {
        assert!(ContentDigest::parse("balls").is_err());
        assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdef").is_ok());
        assert!(ContentDigest::parse("-balls:0123456789abcdef0123456789abcdef").is_err());
        assert!(ContentDigest::parse("--balls:0123456789abcdef0123456789abcdef").is_err());
        assert!(
            ContentDigest::parse("b_b+b+b+b+b+b.balllllls:0123456789abcdef0123456789abcdef")
                .is_ok()
        );
        assert!(
            ContentDigest::parse("b_b+b+b++b+b.balllllls:0123456789abcdef0123456789abcdef")
                .is_err()
        );
        assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdef").is_ok());
        assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdeg").is_err());
        assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdefF").is_ok());
        assert!(
            ContentDigest::parse("ball.ball.ball.balls:0123456789abcdef0123456789abcdef").is_ok()
        );
        assert!(ContentDigest::parse("0123456789abcdef0123456789abcdef").is_err());
        assert!(ContentDigest::parse(":0123456789abcdef0123456789abcdef").is_err());
        assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcde").is_err());
        assert!(ContentDigest::parse("b9:0123456789abcdef0123456789abcdef").is_ok());
        assert!(ContentDigest::parse("b:0123456789abcdef0123456789abcdef").is_ok());
        assert!(ContentDigest::parse("9:0123456789abcdef0123456789abcdef").is_err());
        assert!(ContentDigest::parse(" balls:0123456789abcdef0123456789abcdef").is_err());
        assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdef ").is_err());
    }

    #[test]
    fn parse_repository_name() {
        assert!(Repository::parse("").is_err());
        assert!(Repository::parse("/").is_err());
        assert!(Repository::parse("blah").is_ok());
        assert!(Repository::parse("blah.ok").is_ok());
        assert!(Repository::parse("blah..ok").is_err());
        assert!(Repository::parse(".ok").is_err());
        assert!(Repository::parse("blah/blah.ok").is_ok());
        assert!(Repository::parse("blah/blah..ok").is_err());
        assert!(Repository::parse("blah/.ok").is_err());
        assert!(Repository::parse("/blah").is_err());
        assert!(Repository::parse("blah/").is_err());
        assert!(Repository::parse("blah//blah").is_err());
        assert!(Repository::parse("boring/strings").is_ok());
        assert!(Repository::parse("a").is_ok());
    }
}
