use http::{HeaderMap, StatusCode, Version};
use hyper::body::Bytes;
use serde::{Deserialize, Serialize, Serializer};
use serde::de::{DeserializeOwned, Error};
use serde::ser::SerializeMap;

use crate::{InMemoryBody, InMemoryResult, Response};
use crate::http::SortedSerializableHeaders;
use crate::response::ResponseParts;

pub type InMemoryResponse = Response<InMemoryBody>;

impl InMemoryResponse {
    pub fn new(status: StatusCode, headers: HeaderMap, body: InMemoryBody) -> Self {
        Self::from_parts(ResponseParts {
            status,
            headers,
            version: Version::default(),
        }, body)
    }

    pub fn text(self) -> crate::Result<String> {
        self.body.text()
    }

    pub fn json<U: DeserializeOwned>(self) -> InMemoryResult<U> {
        self.body.json()
    }

    pub fn bytes(self) -> crate::Result<Bytes> {
        self.body.bytes()
    }
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

impl Clone for InMemoryResponse {
    fn clone(&self) -> Self {
        Self {
            parts: self.parts.clone(),
            body: self.body.clone(),
        }
    }
}

