use async_trait::async_trait;
use http::Response;
use hyper::body::Bytes;
use serde::de::DeserializeOwned;

pub use memory::*;

use crate::body::Body;
use crate::Result;

mod memory;

pub(crate) async fn response_into_content(res: Response<Body>) -> Result<InMemoryResponse> {
    let (parts, body) = res.into_parts();
    let content_type = parts.headers.get(hyper::header::CONTENT_TYPE);
    let body = body.into_content_type(content_type).await?;
    Ok(InMemoryResponse::from_parts(parts, body))
}

pub(crate) fn mem_response_into_hyper(res: InMemoryResponse) -> Response<Body> {
    let (parts, body) = res.into_parts();
    let body = body.into();
    Response::from_parts(parts, body)
}

#[async_trait]
pub trait ResponseExt where Self: Sized {
    fn error_for_status(self) -> Result<Self>;
    async fn text(self) -> Result<String>;
    async fn json<U: DeserializeOwned>(self) -> Result<U>;
    /// Get body as bytes.
    async fn bytes(self) -> Result<Bytes>;
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

    async fn text(self) -> Result<String> {
        let (_, body) = self.into_parts();
        let body = body.into_memory().await?;
        body.text()
    }

    async fn json<U: DeserializeOwned>(self) -> Result<U> {
        let (_, body) = self.into_parts();
        let body = body.into_memory().await?;
        body.json().map_err(Into::into)
    }

    /// Get body as bytes.
    async fn bytes(self) -> Result<Bytes> {
        let (_, body) = self.into_parts();
        let body = body.into_memory().await?;
        body.bytes()
    }

    fn get_cookie(&self, name: &str) -> Option<&str> {
        let value = self.headers().get("set-cookie")?;
        let value = value.to_str().ok()?;
        let cookie = cookie::Cookie::split_parse_encoded(value);
        let cookie = cookie.into_iter()
            .filter_map(|c| c.ok())
            .find(|c| c.name() == name)?;
        cookie.value_raw()
    }
}