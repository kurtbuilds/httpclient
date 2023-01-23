use http::{HeaderMap, StatusCode, Version};
use hyper::body::Bytes;
use serde::{Deserialize, Serialize, Serializer};
use serde::de::{DeserializeOwned, Error};
use serde::ser::SerializeMap;

use crate::{InMemoryBody, InMemoryResult, Response, Result};
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

    pub fn text(self) -> Result<String> {
        self.body.text()
    }

    pub fn json<U: DeserializeOwned>(self) -> InMemoryResult<U> {
        self.body.json()
    }

    pub fn bytes(self) -> Result<Bytes> {
        self.body.bytes()
    }

    pub fn sanitize(&mut self) {
        self.body.sanitize();
    }
}

impl Serialize for InMemoryResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let size = 2 + usize::from(!self.body.is_empty());
        let mut map = serializer.serialize_map(Some(size))?;
        map.serialize_entry("status", &self.status().as_u16())?;
        // BTreeMap is sorted...
        let ordered: std::collections::BTreeMap<_, _> = self.headers().iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .collect();
        map.serialize_entry("headers", &ordered)?;
        map.serialize_entry("body", &self.body)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for InMemoryResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> where {

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
                            headers = Some(map.next_value::<std::collections::BTreeMap<&str, &str>>()?);
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
                let headers = HeaderMap::from_iter(headers.ok_or_else(|| Error::missing_field("headers"))?.iter()
                    .map(|(k, v)| (http::header::HeaderName::from_bytes(k.as_bytes()).unwrap(), http::header::HeaderValue::from_str(v).unwrap()))
                );
                let body = body.ok_or_else(|| Error::missing_field("data"))?;
                Ok(InMemoryResponse::new(status, headers, body))
            }
        }

        deserializer.deserialize_map(InMemoryResponseVisitor)
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