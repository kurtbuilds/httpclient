use http::{Version, StatusCode, HeaderMap};
use hyper::body::Bytes;

use crate::body::{Body, InMemoryBody};
use crate::{InMemoryResult, Result};
use serde::{Serialize, Deserialize, Serializer};

use serde::de::{DeserializeOwned, Error};
use serde::ser::SerializeMap;
use crate::http::SortedSerializableHeaders;

pub type InMemoryResponse = Response<InMemoryBody>;

#[derive(Debug)]
pub struct Response<T = Body> {
    pub parts: ResponseParts,
    pub body: T,
}


impl Serialize for InMemoryResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let size = 2 + usize::from(!self.body.is_empty());
        let mut map = serializer.serialize_map(Some(size))?;
        map.serialize_entry("status", &self.status().as_u16())?;
        map.serialize_entry("headers", &crate::http::SortedSerializableHeaders::from(self.headers()))?;
        if !self.body.is_empty() {
            map.serialize_entry("body", &self.body)?;
        }
        map.end()
    }
}


impl<'de> Deserialize<'de> for InMemoryResponse {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(InMemoryResponseVisitor)
    }
}


struct InMemoryResponseVisitor;

impl<'de> serde::de::Visitor<'de> for InMemoryResponseVisitor {
    type Value = InMemoryResponse;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("A map with the following keys: status, headers, body")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error> where A: serde::de::MapAccess<'de> {
        let mut status = None;
        let mut headers = None;
        let mut body = None;
        while let Some(key) = map.next_key::<std::borrow::Cow<str>>()? {
            match key.as_ref() {
                "status" => {
                    if status.is_some() {
                        return Err(<A::Error as Error>::duplicate_field("status"));
                    }
                    let i = map.next_value::<u16>()?;
                    status = Some(StatusCode::from_u16(i).map_err(|_e|
                        <A::Error as Error>::custom("Invalid value for field `status`.")
                    )?);
                }
                "headers" => {
                    if headers.is_some() {
                        return Err(<A::Error as Error>::duplicate_field("headers"));
                    }
                    headers = Some(map.next_value::<SortedSerializableHeaders>()?);
                }
                "data" | "body" => {
                    if body.is_some() {
                        return Err(<A::Error as Error>::duplicate_field("body"));
                    }
                    body = Some(map.next_value::<InMemoryBody>()?);
                }
                _ => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        let status = status.ok_or_else(|| Error::missing_field("status"))?;
        let headers = headers.ok_or_else(|| Error::missing_field("headers"))?;
        let body = body.ok_or_else(|| Error::missing_field("data"))?;
        Ok(InMemoryResponse::new(status, headers.into(), body))
    }
}

#[derive(Debug, Clone)]
pub struct ResponseParts {
    pub version: Version,
    pub status: StatusCode,
    pub headers: HeaderMap,
}

impl InMemoryResponse {
    pub fn new(status: StatusCode, headers: HeaderMap, body: InMemoryBody) -> Self {
        Self::from_parts(ResponseParts {
            status,
            headers,
            version: Version::default(),
        }, body)
    }

    pub fn text(self) -> Result<String> {
        self.body.text()
    }

    pub fn json<U: DeserializeOwned>(self) -> InMemoryResult<U> {
        self.body.json()
    }

    pub fn bytes(self) -> Result<Bytes> {
        self.body.bytes()
    }
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
        Self {
            parts,
            body,
        }
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
        let cookie = cookie.into_iter()
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


impl Clone for InMemoryResponse {
    fn clone(&self) -> Self {
        Self {
            parts: self.parts.clone(),
            body: self.body.clone(),
        }
    }
}