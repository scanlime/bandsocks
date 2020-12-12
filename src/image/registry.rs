use crate::errors::ImageError;
use regex::Regex;
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
    str,
    str::FromStr,
};

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

    pub(crate) fn regex_str() -> &'static str {
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
