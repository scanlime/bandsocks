use crate::errors::ImageError;
use regex::Regex;
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
    hash::{Hash, Hasher},
    str,
    str::FromStr,
};

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

    pub(crate) fn regex_str() -> &'static str {
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
