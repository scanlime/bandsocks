use crate::{
    errors::ImageError,
    image::{ContentDigest, Tag},
};
use std::{
    cmp::{Ord, PartialOrd},
    fmt,
    hash::Hash,
    str,
    str::FromStr,
};

/// Either an image tag or a content digest
///
/// An [crate::image::ImageName] includes an optional tag and an optional
/// content digest. Only the most specific available version is used to actually
/// download an image, though. Any [crate::image::ImageName] can be resolved
/// into an [ImageVersion] that is either a digest, a tag, or the special tag
/// "latest".

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

    /// Is this version a content digest?
    pub fn is_content_digest(&self) -> bool {
        match self {
            ImageVersion::Tag(_) => false,
            ImageVersion::ContentDigest(_) => true,
        }
    }

    /// Is this version a tag?
    pub fn is_tag(&self) -> bool {
        match self {
            ImageVersion::Tag(_) => true,
            ImageVersion::ContentDigest(_) => false,
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
