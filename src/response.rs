use http::{Version, StatusCode, HeaderMap};
use hyper::body::Bytes;

use crate::body::{Body, InMemoryBody};
use crate::{Result};
use serde::{Serialize, Deserialize, Serializer};

use serde::de::{DeserializeOwned, Error};
use serde::ser::SerializeMap;
use crate::http::SortedSerializableHeaders;

pub type InMemoryResponse = Response<InMemoryBody>;

#[derive(Debug)]
pub struct Response<T = Body> {
    pub version: Version,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: T,
}


impl Serialize for InMemoryResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let size = 2 + if self.body.is_empty() { 0 } else { 1 };
        let mut map = serializer.serialize_map(Some(size))?;
        map.serialize_entry("status", &self.status.as_u16())?;
        map.serialize_entry("headers", &crate::http::SortedSerializableHeaders::from(&self.headers))?;
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
        Ok(InMemoryResponse {
            version: Default::default(),
            status,
            headers: headers.into(),
            body,
        })
    }
}

impl Response {
    pub(crate) async fn into_memory(self) -> Result<InMemoryResponse> {
        let content_type = self.headers.get(hyper::header::CONTENT_TYPE);
        let body = self.body.into_memory(content_type).await?;
        Ok(InMemoryResponse {
            status: self.status,
            version: self.version,
            headers: self.headers,
            body,
        })
    }

    pub async fn text(self) -> Result<String> {
        self.body.into_text().await
    }

    pub async fn json<U: DeserializeOwned>(self) -> Result<U> {
        self.body.into_json::<U>().await
    }

    /// Get body as bytes.
    pub async fn bytes(self) -> Result<Bytes> {
        self.body.into_bytes().await
    }
}

impl<T> Response<T> {
    pub fn error_for_status(self) -> std::result::Result<Self, crate::Error<Body>> {
        let status = self.status;
        if status.is_server_error() || status.is_client_error() {
            Err(crate::Error::HttpError(self))
        } else {
            Ok(self)
        }
    }
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn get_cookie(&self, name: &str) -> Option<&str> {
        self.headers.get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                let cookies = basic_cookies::Cookie::parse(v).ok()?;
                cookies.into_iter().find(|c| c.get_name() == name).map(|c| c.get_value())
            })
    }

}

impl From<InMemoryResponse> for Response {
    fn from(value: InMemoryResponse) -> Self {
        Self {
            status: value.status,
            version: value.version,
            headers: value.headers,
            body: value.body.into(),
        }
    }
}

impl From<http::Response<hyper::Body>> for Response {
    fn from(value: http::Response<hyper::Body>) -> Self {
        let (parts, body) = value.into_parts();
        Self {
            status: parts.status,
            version: parts.version,
            headers: parts.headers,
            body: Body::Hyper(body),
        }
    }
}


impl Clone for InMemoryResponse {
    fn clone(&self) -> Self {
        Self {
            status: self.status,
            version: self.version,
            headers: self.headers.clone(),
            body: self.body.clone(),
        }
    }
}