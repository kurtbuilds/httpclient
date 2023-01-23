use std::str::FromStr;

use http::{HeaderMap, Method, Uri};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;
use serde::ser::SerializeMap;

use crate::{InMemoryBody, Request, Result};
use crate::sanitize::sanitize_headers;

pub type InMemoryRequest = Request<InMemoryBody>;


impl InMemoryRequest {
    /// Attempt to clear sensitive information from the request.
    pub fn sanitize(&mut self) {
        sanitize_headers(&mut self.headers);
        self.body.sanitize();
    }
}

impl Clone for InMemoryRequest {
    fn clone(&self) -> Self {
        Self {
            method: self.method.clone(),
            uri: self.uri.clone(),
            version: self.version,
            headers: self.headers.clone(),
            body: self.body.clone(),
        }
    }
}


impl std::hash::Hash for Request<InMemoryBody> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // method
        self.method.hash(state);
        // url, contains query params.
        self.uri.hash(state);
        // headers, sorted
        // let mut sorted = self.headers().iter()
        //     .map(|(k, v)| (k.as_str(), v.as_bytes()))
        //     .collect::<Vec<(&str, &[u8])>>();
        // sorted.sort();
        // sorted.into_iter().for_each(|(k, v)| {
        //     k.hash(state);
        //     v.hash(state);
        // });
        // body
        self.body.hash(state);
    }
}

impl PartialEq<Self> for Request<InMemoryBody> {
    fn eq(&self, other: &Self) -> bool {
        if !(self.method == other.method
            && self.uri == other.uri
            // && self.headers == other.headers
        ) {
            return false;
        }
        match (&self.body, &other.body) {
            (InMemoryBody::Empty, InMemoryBody::Empty) => true,
            (InMemoryBody::Text(ref a), InMemoryBody::Text(ref b)) => a == b,
            (InMemoryBody::Bytes(ref a), InMemoryBody::Bytes(ref b)) => a == b,
            (InMemoryBody::Json(ref a), InMemoryBody::Json(ref b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for InMemoryRequest {}

impl Serialize for InMemoryRequest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let size = 3 + usize::from(!self.body.is_empty());
        let mut map = serializer.serialize_map(Some(size))?;
        map.serialize_entry("method", &self.method.as_str())?;
        map.serialize_entry("url", &self.uri.to_string().as_str())?;
        let ordered: std::collections::BTreeMap<_, _> = self.headers().iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .collect();
        map.serialize_entry("headers", &ordered)?;
        if !self.body.is_empty() {
            map.serialize_entry("body", &self.body)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for InMemoryRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        pub struct InMemoryRequestVisitor;

        impl<'de> serde::de::Visitor<'de> for InMemoryRequestVisitor {
            type Value = InMemoryRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("A map with the following keys: method, url, headers, body")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error> where A: serde::de::MapAccess<'de> {
                use std::collections::BTreeMap;
                use std::borrow::Cow;
                use http::header::{HeaderName, HeaderValue};
                let mut method = None;
                let mut url = None;
                let mut headers = None;
                let mut body = None;
                while let Some(key) = map.next_key::<Cow<str>>()? {
                    match key.as_ref() {
                        "method" => {
                            if method.is_some() {
                                return Err(<A::Error as Error>::duplicate_field("method"));
                            }
                            let s = map.next_value::<String>()?;
                            method = Some(Method::from_str(&s).map_err(|_e|
                                <A::Error as Error>::custom("Invalid value for field `method`.")
                            )?);
                        }
                        "url" => {
                            if url.is_some() {
                                return Err(<A::Error as Error>::duplicate_field("url"));
                            }
                            let s = map.next_value::<String>()?;
                            url = Some(Uri::from_str(&s).map_err(|_e|
                                <A::Error as Error>::custom("Invalid value for field `url`.")
                            )?);
                        }
                        "body" | "data" => {
                            if body.is_some() {
                                return Err(<A::Error as Error>::duplicate_field("data"));
                            }
                            body = Some(map.next_value::<InMemoryBody>()?);
                        }
                        "headers" => {
                            if headers.is_some() {
                                return Err(<A::Error as Error>::duplicate_field("headers"));
                            }
                            headers = Some(map.next_value::<BTreeMap<Cow<'de, str>, Cow<'de, str>>>()?);
                        }
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                let method = method.ok_or_else(|| Error::missing_field("method"))?;
                let url = url.ok_or_else(|| Error::missing_field("url"))?;
                let headers = HeaderMap::from_iter(headers.ok_or_else(|| Error::missing_field("headers"))?.iter()
                    .map(|(k, v)| (HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap()))
                );
                let body = body.unwrap_or(InMemoryBody::Empty);
                Ok(InMemoryRequest {
                    method,
                    uri: url,
                    version: Default::default(),
                    headers,
                    body,
                })
            }
        }

        deserializer.deserialize_map(InMemoryRequestVisitor)
    }
}


#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use super::*;


    #[test]
    fn test_request_serialization_roundtrip() {
        #[derive(Serialize, Deserialize, Debug)]
        struct Foobar {
            a: u32,
            b: u32,
        }
        let data = Foobar { a: 1, b: 2 };
        let r1 = Request::build_post("http://example.com/")
            .json(&data)
            .build();
        let s = serde_json::to_string_pretty(&r1).unwrap();
        let r2: InMemoryRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_equal() {
        #[derive(Serialize, Deserialize, Debug)]
        struct Foobar {
            a: u32,
            b: u32,
        }
        let data = Foobar { a: 1, b: 2 };
        let original = Request::build_post("https://example.com/")
            .header("content-type", "application/json")
            .header("secret", "will-get-sanitized")
            .json(&data)
            .build();
        let mut sanitized = original.clone();
        sanitized.sanitize();
        assert_eq!(original, sanitized);
        assert_eq!(original.header("secret").unwrap(), "will-get-sanitized");
        assert_eq!(sanitized.header("secret").unwrap(), "**********");
        let h1 = {
            let mut s = DefaultHasher::new();
            original.hash(&mut s);
            s.finish()
        };
        let h2 = {
            let mut s = DefaultHasher::new();
            sanitized.hash(&mut s);
            s.finish()
        };
        assert_eq!(h1, h2);
    }
}