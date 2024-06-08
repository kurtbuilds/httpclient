use serde::de::Error;

use crate::{InMemoryBody, Request};

pub type InMemoryRequest = Request<InMemoryBody>;

pub mod serde_request {
    use std::str::FromStr;

    use http::{HeaderMap, Method, Request, Uri};
    use serde::{Deserializer, Serializer};
    use serde::de::Error;
    use serde::ser::SerializeMap;

    use crate::{InMemoryBody, InMemoryRequest};

    pub fn serialize<S>(req: &InMemoryRequest, serializer: S) -> crate::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let size = 3 + usize::from(!req.body().is_empty());
        let mut map = serializer.serialize_map(Some(size))?;
        map.serialize_entry("method", &req.method().as_str())?;
        map.serialize_entry("url", &req.uri().to_string().as_str())?;
        let ordered: std::collections::BTreeMap<_, _> = req.headers().iter().map(|(k, v)| (k.as_str(), v.to_str().unwrap())).collect();
        map.serialize_entry("headers", &ordered)?;
        if !req.body().is_empty() {
            map.serialize_entry("body", &req.body())?;
        }
        map.end()
    }

    struct InMemoryRequestVisitor;

    impl<'de> serde::de::Visitor<'de> for InMemoryRequestVisitor {
        type Value = InMemoryRequest;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("A map with the following keys: method, url, headers, body")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            use http::header::{HeaderName, HeaderValue};
            use std::borrow::Cow;
            use std::collections::BTreeMap;
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
                        method = Some(Method::from_str(&s).map_err(|_e| <A::Error as Error>::custom("Invalid value for field `method`."))?);
                    }
                    "url" => {
                        if url.is_some() {
                            return Err(<A::Error as Error>::duplicate_field("url"));
                        }
                        let s = map.next_value::<String>()?;
                        url = Some(Uri::from_str(&s).map_err(|_e| <A::Error as Error>::custom("Invalid value for field `url`."))?);
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
            let headers = HeaderMap::from_iter(
                headers
                    .ok_or_else(|| Error::missing_field("headers"))?
                    .iter()
                    .map(|(k, v)| (HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap())),
            );
            let body = body.unwrap_or(InMemoryBody::Empty);
            let mut b = Request::builder().method(method).uri(url);
            *b.headers_mut().unwrap() = headers;
            b.body(body).map_err(|e| <A::Error as Error>::custom(format!("Invalid request: {}", e)))
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> crate::Result<InMemoryRequest, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(InMemoryRequestVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};
    use std::io::BufWriter;

    use serde::{Deserialize, Serialize};

    use crate::recorder::HashableRequest;

    use super::*;

    #[test]
    fn test_request_serialization_roundtrip() {
        #[derive(Serialize, Deserialize, Debug)]
        struct Foobar {
            a: u32,
            b: u32,
        }
        let data = Foobar { a: 1, b: 2 };
        let r1 = Request::builder()
            .method("POST")
            .uri("http://example.com/")
            .body(InMemoryBody::Json(serde_json::to_value(&data).unwrap()))
            .unwrap();
        let serialized = BufWriter::new(Vec::new());
        let mut serializer = serde_json::Serializer::new(serialized);
        serde_request::serialize(&r1, &mut serializer).unwrap();
        let s = serializer.into_inner().into_inner().unwrap();

        let mut deserializer = serde_json::Deserializer::from_slice(&s);
        let r2 = serde_request::deserialize(&mut deserializer).unwrap();
        assert_eq!(HashableRequest(r1), HashableRequest(r2));
    }
}
