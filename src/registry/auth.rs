use crate::{errors::ImageError, image::Registry};
use http::header::HeaderValue;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Auth {
    logins: HashMap<Registry, (String, String)>,
}

impl Auth {
    pub fn new() -> Self {
        Auth {
            logins: HashMap::new(),
        }
    }

    pub fn login(&mut self, registry: Registry, username: String, password: String) {
        self.logins.insert(registry, (username, password));
    }

    pub async fn authenticate_for(
        &self,
        req: &reqwest::Client,
        auth_header: &HeaderValue,
    ) -> Result<(), ImageError> {
        log::error!("{:?}", auth_header);
        Ok(())
    }
}
