use crate::errors::ImageError;
use regex::Regex;
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
    hash::{Hash, Hasher},
    str,
    str::FromStr,
};

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
    /// # use bandsocks::Repository;
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

    pub(crate) fn regex_str() -> &'static str {
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
