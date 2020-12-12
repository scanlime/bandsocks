use crate::errors::ImageError;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
    str,
    str::FromStr,
};

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
    /// # use bandsocks::ContentDigest;
    /// let digest = ContentDigest::from_content(b"cat");
    /// assert_eq!(digest.as_str(), "sha256:77af778b51abd4a3c51c5ddd97204a9c3ae614ebccb75a606c3b6865aed6744e");
    /// ```
    pub fn from_content(content_bytes: &[u8]) -> Self {
        ContentDigest::from_parts("sha256", &Sha256::digest(content_bytes)).unwrap()
    }

    /// Parse a [prim@str] as a [ContentDigest]
    ///
    /// ```
    /// # use bandsocks::ContentDigest;
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

    pub(crate) fn regex_str() -> &'static str {
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
