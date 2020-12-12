use crate::{
    errors::ImageError,
    image::{ContentDigest, ImageVersion, Registry, Repository, Tag},
};
use regex::Regex;
use std::{
    cmp::{Ordering, PartialOrd},
    fmt,
    hash::{Hash, Hasher},
    io::Write,
    ops::Range,
    str,
    str::FromStr,
};

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

    /// Create a new [ImageName] which includes the actual content digest we
    /// found
    ///
    /// If the name already includes a digest, it is validated. On mismatch, an
    /// appropriate error will be returned. On match, or if no validation
    /// was requested, this returns a new specific [ImageName] with the provided
    /// digest.
    pub fn with_found_digest(&self, found_digest: &ContentDigest) -> Result<ImageName, ImageError> {
        match self.content_digest() {
            None => ImageName::from_parts(
                self.registry_str(),
                self.repository_str(),
                self.tag_str(),
                Some(found_digest.as_str()),
            ),
            Some(image_digest) if &image_digest == found_digest => Ok(self.clone()),
            Some(image_digest) => {
                return Err(ImageError::ContentDigestMismatch {
                    expected: image_digest.clone(),
                    found: found_digest.clone(),
                })
            }
        }
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
