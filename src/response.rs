use http::{HeaderMap, StatusCode, Version};
use hyper::body::Bytes;
pub use memory::*;
use serde::de::DeserializeOwned;

use crate::body::Body;
use crate::Result;

mod memory;

#[derive(Debug)]
pub struct Response<T = Body> {
    pub parts: ResponseParts,
    pub body: T,
}

impl Response {
    pub(crate) async fn into_content(self) -> Result<InMemoryResponse> {
        let (parts, body) = self.into_parts();
        let content_type = parts.headers.get(hyper::header::CONTENT_TYPE);
        let body = body.into_content_type(content_type).await?;
        Ok(InMemoryResponse::from_parts(parts, body))
    }

    pub fn error_for_status(self) -> Result<Self> {
        let status = self.status();
        if status.is_server_error() || status.is_client_error() {
            Err(crate::Error::HttpError(self))
        } else {
            Ok(self)
        }
    }

    pub async fn text(self) -> Result<String> {
        let body = self.body.into_memory().await?;
        body.text()
    }

    pub async fn json<U: DeserializeOwned>(self) -> Result<U> {
        let body = self.body.into_memory().await?;
        body.json().map_err(Into::into)
    }

    /// Get body as bytes.
    pub async fn bytes(self) -> Result<Bytes> {
        let body = self.body.into_memory().await?;
        body.bytes()
    }
}

impl<T> Response<T> {
    pub fn from_parts(parts: ResponseParts, body: T) -> Self {
        Self { parts, body }
    }

    pub fn into_parts(self) -> (ResponseParts, T) {
        (self.parts, self.body)
    }

    pub fn status(&self) -> StatusCode {
        self.parts.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.parts.headers
    }

    pub fn get_cookie(&self, name: &str) -> Option<&str> {
        let value = self.parts.headers.get("set-cookie")?;
        let value = value.to_str().ok()?;
        let cookie = cookie::Cookie::split_parse_encoded(value);
        let cookie = cookie
            .into_iter()
            .filter_map(|c| c.ok())
            .find(|c| c.name() == name)?;
        cookie.value_raw()
    }
}

impl From<InMemoryResponse> for Response {
    fn from(value: InMemoryResponse) -> Self {
        let (parts, body) = value.into_parts();
        Self {
            parts,
            body: body.into(),
        }
    }
}

impl From<http::Response<hyper::Body>> for Response {
    fn from(value: http::Response<hyper::Body>) -> Self {
        let (parts, body) = value.into_parts();
        Self {
            parts: ResponseParts {
                status: parts.status,
                headers: parts.headers,
                version: parts.version,
            },
            body: Body::Hyper(body),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResponseParts {
    pub version: Version,
    pub status: StatusCode,
    pub headers: HeaderMap,
}
