use http::{HeaderMap, Response, StatusCode};
use hyper::body::Bytes;
use serde::de::{DeserializeOwned, Error};

use crate::{InMemoryBody, Result};
use crate::sanitize::sanitize_headers;

pub type InMemoryResponse = Response<InMemoryBody>;

pub trait InMemoryResponseExt {
    fn new(status: StatusCode, headers: HeaderMap, body: InMemoryBody) -> Self;
    fn text(self) -> Result<String>;
    fn json<U: DeserializeOwned>(self) -> serde_json::Result<U>;
    fn bytes(self) -> Result<Bytes>;
    /// Attempt to clear sensitive information from the response.
    fn sanitize(&mut self);
}

impl InMemoryResponseExt for InMemoryResponse {
    fn new(status: StatusCode, headers: HeaderMap, body: InMemoryBody) -> Self {
        let mut b = http::response::Builder::new()
            .status(status);
        let h = b.headers_mut().unwrap();
        *h = headers;
        b.body(body).unwrap()
    }

    fn text(self) -> Result<String> {
        let (_, body) = self.into_parts();
        body.text()
    }

    fn json<U: DeserializeOwned>(self) -> serde_json::Result<U> {
        let (_, body) = self.into_parts();
        body.json()
    }

    fn bytes(self) -> Result<Bytes> {
        let (_, body) = self.into_parts();
        body.bytes()
    }

    /// Attempt to clear sensitive information from the response.
    fn sanitize(&mut self) {
        let h = self.headers_mut();
        sanitize_headers(h);
        self.body_mut().sanitize();
    }
}


pub(crate) fn clone_inmemory_response(res: &InMemoryResponse) -> InMemoryResponse {
    let (mut parts, _) = Response::new(()).into_parts();
    parts.headers = res.headers().clone();
    parts.status = res.status();
    parts.version = res.version();
    let body = res.body().clone();
    Response::from_parts(parts, body)
}

pub mod serde_response {
    use std::collections::BTreeMap;

    use serde::Deserializer;
    use serde::ser::SerializeStruct;

    use super::*;

    pub fn serialize<S>(v: &InMemoryResponse, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
    {
        let size = 2 + usize::from(!v.body().is_empty());
        let mut map = serializer.serialize_struct("InMemoryResponse", size)?;
        map.serialize_field("status", &v.status().as_u16())?;
        let ordered: BTreeMap<_, _> = v.headers().iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .collect();
        map.serialize_field("headers", &ordered)?;
        map.serialize_field("body", &v.body())?;
        map.end()
    }

    struct InMemoryResponseVisitor;

    impl<'de> serde::de::Visitor<'de> for InMemoryResponseVisitor {
        type Value = InMemoryResponse;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("A map with the following keys: status, headers, body")
        }

        fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error> where A: serde::de::MapAccess<'de> {
            use std::borrow::Cow;
            use hyper::header::{HeaderName, HeaderValue};

            let mut status = None;
            let mut headers = None;
            let mut body = None;
            while let Some(key) = map.next_key::<Cow<str>>()? {
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
                        headers = Some(map.next_value::<BTreeMap<Cow<'de, str>, Cow<'de, str>>>()?);
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
                .map(|(k, v)| (HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap()))
            );
            let body = body.ok_or_else(|| Error::missing_field("data"))?;
            let mut b = http::response::Builder::new()
                .status(status);
            let h = b.headers_mut().unwrap();
            *h = headers;
            Ok(b.body(body).unwrap())
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<InMemoryResponse, D::Error>
        where
            D: Deserializer<'de>,
    {
        deserializer.deserialize_map(InMemoryResponseVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufWriter;

    use serde_json::json;

    use super::*;

    #[test]
    fn test_serialize() {
        let mut res = http::response::Builder::new()
            .status(StatusCode::OK)
            .body(InMemoryBody::new_json(json!({
                "Password": "hunter2",
                "email": "amazing",
            }))).unwrap();
        res.sanitize();
        let serialized = BufWriter::new(Vec::new());
        let mut serializer = serde_json::Serializer::new(serialized);
        serde_response::serialize(&res, &mut serializer).unwrap();
        let serialized = String::from_utf8(serializer.into_inner().into_inner().unwrap()).unwrap();
        assert_eq!(serialized, r#"{"status":200,"headers":{},"body":{"Password":"**********","email":"amazing"}}"#);
    }
}