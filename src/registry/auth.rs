use crate::{errors::ImageError, image::Registry};
use regex::Regex;
use reqwest::{RequestBuilder, Url};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Auth {
    logins: HashMap<Registry, Login>,
    tokens: HashMap<Registry, Token>,
}

#[derive(Clone)]
struct Login {
    username: String,
    password: Option<String>,
}

impl Auth {
    pub fn new() -> Self {
        Auth {
            logins: HashMap::new(),
            tokens: HashMap::new(),
        }
    }

    pub fn login(&mut self, registry: Registry, username: String, password: Option<String>) {
        self.logins.insert(registry, Login { username, password });
    }

    pub fn include_token(&self, registry: &Registry, req: RequestBuilder) -> RequestBuilder {
        match self.tokens.get(registry) {
            Some(token_struct) => {
                log::debug!("using token for {}", registry);
                req.bearer_auth(&token_struct.token)
            }
            None => req,
        }
    }

    /// Reference: <https://docs.docker.com/registry/spec/auth/token/>
    pub async fn authenticate_for(
        &mut self,
        registry: &Registry,
        req: &reqwest::Client,
        auth_header: &str,
    ) -> Result<(), ImageError> {
        let challenge = BearerChallenge::parse(auth_header)?;
        log::debug!("login challenge for {}, {:?}", registry, challenge);
        let req = req
            .get(challenge.realm)
            .query(&[("service", challenge.service), ("scope", challenge.scope)]);
        let req = match self.logins.get(registry) {
            Some(login) => req.basic_auth(&login.username, login.password.as_ref()),
            None => req,
        };
        let response: Token = req.send().await?.error_for_status()?.json().await?;
        log::debug!("received token for {}", registry);
        self.tokens.insert(registry.clone(), response);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct BearerChallenge {
    realm: Url,
    service: String,
    scope: String,
}

#[derive(Clone, Deserialize)]
struct Token {
    token: String,
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
                /* -- */ ")",
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
