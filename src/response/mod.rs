use async_trait::async_trait;
use http::Response;
use hyper::body::Bytes;
use serde::de::DeserializeOwned;

pub use in_memory_ext::*;

use crate::body::Body;
use crate::{InMemoryResult, Result};

mod in_memory_ext;

#[async_trait]
pub trait ResponseExt
where
    Self: Sized,
{
    fn error_for_status(self) -> Result<Self>;
    async fn text(self) -> InMemoryResult<String>;
    async fn json<U: DeserializeOwned>(self) -> InMemoryResult<U>;
    /// Get body as bytes.
    async fn bytes(self) -> InMemoryResult<Bytes>;
    fn get_cookie(&self, name: &str) -> Option<&str>;
}

#[async_trait]
impl ResponseExt for Response<Body> {
    fn error_for_status(self) -> Result<Self> {
        let status = self.status();
        if status.is_server_error() || status.is_client_error() {
            Err(crate::Error::HttpError(self))
        } else {
            Ok(self)
        }
    }

    async fn text(self) -> InMemoryResult<String> {
        let (_, body) = self.into_parts();
        let body = body.into_memory().await?;
        body.into_text()
    }

    async fn json<U: DeserializeOwned>(self) -> InMemoryResult<U> {
        let (_, body) = self.into_parts();
        let body = body.into_memory().await?;
        body.json().map_err(Into::into)
    }

    /// Get body as bytes.
    async fn bytes(self) -> InMemoryResult<Bytes> {
        let (_, body) = self.into_parts();
        let body = body.into_memory().await?;
        body.bytes()
    }

    fn get_cookie(&self, name: &str) -> Option<&str> {
        let value = self.headers().get("set-cookie")?;
        let value = value.to_str().ok()?;
        let cookie = cookie::Cookie::split_parse_encoded(value);
        let cookie = cookie.into_iter().filter_map(std::result::Result::ok).find(|c| c.name() == name)?;
        cookie.value_raw()
    }
}
