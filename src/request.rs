use std::borrow::Cow;
use std::future::IntoFuture;
use std::str::FromStr;

use futures::future::BoxFuture;
use http::{HeaderMap, HeaderValue, Version};
use http::header::HeaderName;
use http::uri::PathAndQuery;
use hyper::{Method, Uri};
use serde::{Deserialize, Deserializer, Serialize};
use serde::de::{Error};
use serde::ser::SerializeMap;
use serde::ser::Serializer;
use serde_json::Value;

use crate::{Body, Result, Response};
use crate::body::{InMemoryBody};
use crate::client::Client;
use crate::middleware::Next;
use crate::response::{InMemoryResponse};

pub type InMemoryRequest = Request<InMemoryBody>;

#[derive(Debug)]
pub struct Request<T = Body> {
    method: Method,
    uri: Uri,
    version: Version,
    headers: HeaderMap,
    body: T,
}

impl Serialize for InMemoryRequest {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
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

struct InMemoryRequestVisitor;


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

impl<T> Request<T> {
    pub fn host(&self) -> &str {
        self.uri.host().unwrap_or("")
    }
    pub fn version(&self) -> Version {
        self.version
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    pub fn url(&self) -> &Uri {
        &self.uri
    }

    pub fn path(&self) -> &str {
        self.uri.path()
    }

    pub fn body(&self) -> &T {
        &self.body
    }

    pub fn body_mut(&mut self) -> &mut T {
        &mut self.body
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn headers_mut(&mut self) -> &HeaderMap {
        &mut self.headers
    }

    pub fn set_url(mut self, url: Uri) -> Self {
        self.uri = url;
        self
    }
}

impl Request {
    pub async fn into_memory(self) -> Result<InMemoryRequest> {
        let body = self.body.into_memory().await?;
        Ok(Request {
            method: self.method,
            uri: self.uri,
            version: self.version,
            headers: self.headers,
            body,
        })
    }

    pub fn build_post(url: &str) -> RequestBuilder<(), InMemoryBody> {
        RequestBuilder::new(&(), Method::POST, Uri::from_str(url).expect("Invalid URL"))
    }

    pub fn build_get(url: &str) -> RequestBuilder<(), InMemoryBody> {
        RequestBuilder::new(&(), Method::GET, Uri::from_str(url).expect("Invalid URL"))
    }

    pub fn build_patch(url: &str) -> RequestBuilder<(), InMemoryBody> {
        RequestBuilder::new(&(), Method::PATCH, Uri::from_str(url).expect("Invalid URL"))
    }

    pub fn build_delete(url: &str) -> RequestBuilder<(), InMemoryBody> {
        RequestBuilder::new(&(), Method::DELETE, Uri::from_str(url).expect("Invalid URL"))
    }
}

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


impl From<InMemoryRequest> for Request {
    fn from(val: InMemoryRequest) -> Self {
        Request {
            method: val.method,
            uri: val.uri,
            version: val.version,
            headers: val.headers,
            body: val.body.into(),
        }
    }
}

impl Into<hyper::Request<hyper::Body>> for Request {
    fn into(self) -> http::Request<hyper::Body> {
        let mut builder = http::Request::builder()
            .version(self.version)
            .method(self.method)
            .uri(self.uri);
        for (key, value) in self.headers.into_iter().filter_map(|(k, v)| Some((k?, v))) {
            builder = builder.header(key, value);
        }
        builder
            .body(self.body.into())
            .unwrap()
    }
}


#[derive(Debug)]
pub struct RequestBuilder<'a, C = Client, B = InMemoryBody> {
    client: &'a C,

    pub version: Version,
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: Option<B>,
}

impl<'a, C> RequestBuilder<'a, C> {
    pub fn new(client: &'a C, method: Method, uri: Uri) -> RequestBuilder<'a, C, InMemoryBody> {
        RequestBuilder {
            client,
            version: Default::default(),
            method,
            uri,
            headers: Default::default(),
            body: Default::default(),
        }
    }
    /// Overwrite the current body with the provided JSON object.
    pub fn set_json<S: Serialize>(mut self, obj: S) -> Self {
        self.body = Some(InMemoryBody::Json(serde_json::to_value(obj).unwrap()));
        self.headers.entry(&hyper::header::CONTENT_TYPE).or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        self.headers.entry(hyper::header::ACCEPT).or_insert(HeaderValue::from_static("application/json"));
        self
    }

    /// Add the provided JSON object to the current body.
    pub fn json<S: Serialize>(mut self, obj: S) -> Self {
        match self.body {
            None => {
                self.set_json(obj)
            }
            Some(InMemoryBody::Json(Value::Object(ref mut body))) => {
                if let Value::Object(obj) = serde_json::to_value(obj).unwrap() {
                    body.extend(obj.into_iter());
                } else {
                    panic!("Tried to push a non-object to a json body.");
                }
                self
            }
            _ => panic!("Tried to call .json() on a non-json body. Use .set_json if you need to force a json body."),
        }
    }

    /// Sets content-type to `application/octet-stream` and the body to the supplied bytes.
    pub fn bytes(mut self, bytes: Vec<u8>) -> Self {
        self.body = Some(InMemoryBody::Bytes(bytes));
        self.headers.entry(hyper::header::CONTENT_TYPE).or_insert(HeaderValue::from_static("application/octet-stream"));
        self
    }

    /// Sets content-type to `text/plain` and the body to the supplied text.
    pub fn text(mut self, text: String) -> Self {
        self.body = Some(InMemoryBody::Text(text));
        self.headers.entry(hyper::header::CONTENT_TYPE).or_insert(HeaderValue::from_static("text/plain"));
        self
    }
}

impl<'a> RequestBuilder<'a> {
    pub async fn send(self) -> Result<Response> {
        let next = Next {
            client: self.client,
            middlewares: self.client.middlewares.as_slice(),
        };
        let request = self.build();
        next.run(request.into()).await
    }

    /// Normally, we have to `await` the body as well. This convenience method makes the body
    /// available immediately.
    pub fn send_awaiting_body(self) -> BoxFuture<'a, Result<InMemoryResponse, crate::Error<InMemoryBody>>> {
        Box::pin(async move {
            let res = self.send().await;
            let res = match res {
                Ok(res) => res,
                Err(e) => return Err(e.into_memory().await),
            };
            let (parts, body) = res.into_parts();
            let body = match body.into_memory().await {
                Ok(body) => body,
                Err(e) => return Err(e.into()),
            };
            let res = Response::from_parts(parts, body);
            if res.status().is_client_error() || res.status().is_server_error() {
                Err(crate::Error::HttpError(res))
            } else {
                Ok(res)
            }
        })
    }
}


impl<'a, C, B: Default> RequestBuilder<'a, C, B> {
    pub fn build(self) -> Request<B> {
        Request {
            method: self.method,
            uri: self.uri,
            version: self.version,
            headers: self.headers,
            body: self.body.unwrap_or_default(),
        }
    }
}

impl<'a, C, B> RequestBuilder<'a, C, B> {
    pub fn for_client(client: &'a C) -> RequestBuilder<'a, C> {
        RequestBuilder {
            client,
            version: Default::default(),
            method: Default::default(),
            uri: Default::default(),
            headers: Default::default(),
            body: Default::default(),
        }
    }

    pub fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn url(mut self, uri: &str) -> Self {
        self.uri = Uri::from_str(uri).expect("Invalid URI");
        self
    }

    pub fn set_headers<S: AsRef<str>, I: Iterator<Item=(S, S)>>(mut self, headers: I) -> Self {
        self.headers = HeaderMap::new();
        self.headers(headers)
    }

    pub fn headers<S: AsRef<str>, I: Iterator<Item=(S, S)>>(mut self, headers: I) -> Self {
        self.headers.extend(headers.map(|(k, v)| (
            HeaderName::from_str(k.as_ref()).unwrap(),
            HeaderValue::from_str(v.as_ref()).unwrap()
        )));
        self
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(
            HeaderName::from_str(key).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
        self
    }

    pub fn cookie(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(
            hyper::header::COOKIE,
            HeaderValue::from_str(&format!("{}={}", key, value)).unwrap(),
        );
        self
    }

    pub fn bearer_auth(mut self, token: &str) -> Self {
        self.headers.insert(
            hyper::header::AUTHORIZATION,
            hyper::header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );
        self
    }

    pub fn token_auth(mut self, token: &str) -> Self {
        self.headers.insert(
            hyper::header::AUTHORIZATION,
            hyper::header::HeaderValue::from_str(&format!("Token {}", token)).unwrap(),
        );
        self
    }

    pub fn basic_auth(mut self, token: &str) -> Self {
        self.headers.insert(
            hyper::header::AUTHORIZATION,
            hyper::header::HeaderValue::from_str(&format!("Basic {}", token)).unwrap(),
        );
        self
    }

    /// Overwrite the query with the provided value.
    pub fn set_query<S: Serialize>(mut self, obj: S) -> Self {
        let query = {
            let val = serde_json::to_value(obj).expect("Failed to serialize query in .set_query");
            let map = val.as_object().expect("object in .set_query was not a Map");
            map.into_iter().map(|(k, v)| {
                let v = match v {
                    Value::String(s) => Cow::Borrowed(s.as_ref()),
                    Value::Number(n) => Cow::Owned(n.to_string()),
                    Value::Bool(b) => Cow::Owned(b.to_string()),
                    Value::Null => Cow::Borrowed(""),
                    _ => panic!("Invalid query value"),
                };
                let v = urlencoding::encode(&v);
                urlencoding::encode(k).to_string() + "=" + &v
            }).collect::<Vec<_>>()
                .join("&")
        };

        let mut parts = std::mem::take(&mut self.uri).into_parts();
        let pq = parts.path_and_query.unwrap();
        let pq = PathAndQuery::from_str(&format!("{}?{}", pq.path(), query)).unwrap();
        parts.path_and_query = Some(pq);
        self.uri = Uri::from_parts(parts).unwrap();
        self
    }

    /// Add a url query parameter, but keep existing parameters.
    /// # Examples
    /// ```
    /// use httpclient::{Client, RequestBuilder, Method};
    /// let client = Client::new();
    /// let mut r = RequestBuilder::new(&client, Method::GET, "http://example.com/foo?a=1".parse().unwrap());
    /// r = r.query("b", "2");
    /// assert_eq!(r.uri.to_string(), "http://example.com/foo?a=1&b=2");
    /// ```
    pub fn query(mut self, k: &str, v: &str) -> Self {
        let mut parts = std::mem::take(&mut self.uri).into_parts();
        let pq = parts.path_and_query.unwrap();
        let pq = PathAndQuery::from_str(match pq.query() {
            Some(q) => format!("{}?{}&{}={}", pq.path(), q, urlencoding::encode(k), urlencoding::encode(v)),
            None => format!("{}?{}={}", pq.path(), urlencoding::encode(k), urlencoding::encode(v)),
        }.as_str()).unwrap();
        parts.path_and_query = Some(pq);
        self.uri = Uri::from_parts(parts).unwrap();
        self
    }

    pub fn content_type(mut self, content_type: &str) -> Self {
        self.headers.insert(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_str(content_type).unwrap(),
        );
        self
    }

    /// Warning: Does not set content-type!
    pub fn body(mut self, body: B) -> Self {
        self.body = Some(body);
        self
    }

    pub fn try_build(self) -> Result<Request<B>, crate::Error> {
        Ok(Request {
            method: self.method,
            uri: self.uri,
            version: self.version,
            headers: self.headers,
            body: self.body.ok_or_else(|| crate::Error::<Body>::Custom("No body set".to_string()))?,
        })
    }
}

impl<'a> IntoFuture for RequestBuilder<'a, Client> {
    type Output = Result<Response>;
    type IntoFuture = BoxFuture<'a, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.send())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use serde_json::json;
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
        let r1 = Request::build_post("https://example.com/")
            .header("content-type", "application/json")
            .json(&data)
            .build();
        let r2 = Request::build_post("https://example.com/")
            .header("content-type", "application/json")
            .json(&data)
            .build();
        assert_eq!(r1, r2);
        let h1 = {
            let mut s = DefaultHasher::new();
            r1.hash(&mut s);
            s.finish()
        };
        let h2 = {
            let mut s = DefaultHasher::new();
            r2.hash(&mut s);
            s.finish()
        };
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_push_query() {
        let mut r1 = Request::build_get("https://example.com/foo/bar");
        r1 = r1.query("a", "b");
        assert_eq!(r1.uri.to_string(), "https://example.com/foo/bar?a=b");
        r1 = r1.query("c", "d");
        assert_eq!(r1.uri.to_string(), "https://example.com/foo/bar?a=b&c=d");
    }

    #[test]
    fn test_query() {
        let r1 = Request::build_get("http://example.com/foo/bar")
            .set_query(HashMap::from([("a", Some("b")), ("c", Some("d")), ("e", None)]));
        assert_eq!(r1.uri.to_string(), "http://example.com/foo/bar?a=b&c=d&e=");
        assert_eq!(r1.build().url().to_string(), "http://example.com/foo/bar?a=b&c=d&e=");
    }

    #[test]
    fn test_client_request() {
        let client = Client::new();
        let _ = client.post("/foo").json(json!({"a": 1}));
    }

    #[test]
    fn test_request_builder() {
        let client = Client::new();
        let _ = RequestBuilder::new(&client, Method::POST, "http://example.com/foo".parse().unwrap());
    }
}