use std::borrow::Cow;
use serde::de::Error;
use http::{HeaderMap, HeaderValue, Method, Uri, Version};
use std::str::FromStr;
use http::header::HeaderName;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::ser::SerializeMap;
use crate::{InMemoryBody, Request};

pub type InMemoryRequest = Request<InMemoryBody>;


impl InMemoryRequest {
    pub fn test(method: &str, url: &str) -> Self {
        Self {
            method: Method::from_str(&method.to_uppercase()).unwrap(),
            uri: Uri::from_str(url).unwrap(),
            version: Default::default(),
            headers: Default::default(),
            body: InMemoryBody::Empty,
        }
    }

    pub fn set_body(mut self, body: InMemoryBody) -> Self {
        self.body = body;
        self
    }

    pub fn set_header(mut self, key: impl Into<HeaderName>, value: impl Into<HeaderValue>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn set_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    pub fn set_version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    pub fn set_method(mut self, method: Method) -> Self {
        self.method = method;
        self
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
        map.serialize_entry("headers", &crate::http::SortedSerializableHeaders::from(&self.headers))?;
        if !self.body.is_empty() {
            map.serialize_entry("body", &self.body)?;
        }
        map.end()
    }
}


impl<'de> Deserialize<'de> for InMemoryRequest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
        where
            D: Deserializer<'de>,
    {
        deserializer.deserialize_map(InMemoryRequestVisitor)
    }
}
pub struct InMemoryRequestVisitor;


impl<'de> serde::de::Visitor<'de> for InMemoryRequestVisitor {
    type Value = InMemoryRequest;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("A map with the following keys: method, url, headers, body")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error> where A: serde::de::MapAccess<'de> {
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
                    let s = map.next_value::<crate::http::SortedSerializableHeaders>()?;
                    headers = Some(s);
                }
                _ => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        let method = method.ok_or_else(|| Error::missing_field("method"))?;
        let url = url.ok_or_else(|| Error::missing_field("url"))?;
        let headers = headers.ok_or_else(|| Error::missing_field("headers"))?;
        let body = body.unwrap_or(InMemoryBody::Empty);
        Ok(InMemoryRequest {
            method,
            uri: url,
            version: Default::default(),
            headers: headers.into(),
            body,
        })
    }
}
