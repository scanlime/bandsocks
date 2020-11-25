use crate::{errors::ImageError, image::Registry};
use regex::Regex;
use std::collections::HashMap;
use url::Url;

#[derive(Clone)]
pub struct Auth {
    logins: HashMap<Registry, Login>,
}

#[derive(Clone)]
struct Login {
    username: String,
    password: String,
}

impl Auth {
    pub fn new() -> Self {
        Auth {
            logins: HashMap::new(),
        }
    }

    pub fn login(&mut self, registry: Registry, username: String, password: String) {
        self.logins.insert(registry, Login { username, password });
    }

    /// Reference: <https://docs.docker.com/registry/spec/auth/token/>
    pub async fn authenticate_for(
        &self,
        registry: &Registry,
        req: &reqwest::Client,
        auth_header: &str,
    ) -> Result<(), ImageError> {
        let login = self.logins.get(registry);
        let challenge = BearerChallenge::parse(auth_header)?;

        log::info!("{} {:?}", login.is_some(), challenge);

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct BearerChallenge {
    realm: Url,
    service: String,
    scope: String,
}

impl BearerChallenge {
    fn parse(auth_header: &str) -> Result<Self, ImageError> {
        lazy_static! {
            static ref RE: Regex = Regex::new(concat!(
                "^\\s*",
                "(?i:bearer)",   // Case-insensitive challenge type
                "(?:",           // multiple unordered parameters
                /* */ "\\s*",
                /* */ "(?:",     // alternative group for the parameters
                /* -- */ "(?:",  // parameter: service
                /* -- -- */ "service=",
                /* -- -- */ "\"(?P<service>",
                /* -- -- -- */ r"[\x20-\x21\x23-\x5B\x5D-\x7E]*", // allowed chars from RFC 6750
                /* -- -- */ ")\"",
                /* -- */ ")|",
                /* -- */ "(?:",  // parameter: scope
                /* -- -- */ "scope=",
                /* -- -- */ "\"(?P<scope>",
                /* -- -- -- */ r"[\x20-\x21\x23-\x5B\x5D-\x7E]*", // allowed chars from RFC 6750
                /* -- -- */ ")\"",
                /* -- */ ")|",
                /* -- */ "(?:",  // parameter: realm
                /* -- -- */ "realm=",
                /* -- -- */ "\"(?P<realm>", // capture quoted string
                /* -- -- -- */ "https://",  // require auth server to be https
                /* -- -- -- */ "[-_.+a-zA-Z:0-9/]+",
                /* -- -- */ ")\"",
                /* -- */ ")|",
                /* */ ")",
                /* */ ",?",      // to keep the parser regular, commas are all optional *shrug*
                ")*$",
            )).unwrap();
        }
        match RE.captures(auth_header).map(|captures| {
            (
                captures.name("service").map(|m| m.as_str().to_owned()),
                captures.name("scope").map(|m| m.as_str().to_owned()),
                captures.name("realm").map(|m| m.as_str().parse::<Url>()),
            )
        }) {
            Some((Some(service), Some(scope), Some(Ok(realm)))) => Ok(BearerChallenge {
                realm,
                service,
                scope,
            }),
            _ => Err(ImageError::UnsupportedAuthentication(
                auth_header.to_string(),
            )),
        }
    }
}
