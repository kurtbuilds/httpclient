use std::borrow::Cow;
use std::future::IntoFuture;
use std::str::FromStr;

use futures::future::BoxFuture;
use http::{HeaderMap, HeaderValue, Version};
use http::header::HeaderName;
use http::uri::PathAndQuery;
use hyper::{Method, Uri};
use serde::{Deserialize, Deserializer, Serialize};
use serde::de::{DeserializeOwned, Error};
use serde::ser::SerializeMap;
use serde::ser::Serializer;
use serde_json::Value;

use crate::{Body, Result, Response};
use crate::body::InMemoryBody;
use crate::client::Client;
use crate::middleware::Next;
use crate::response::{InMemoryResponse};

pub type InMemoryRequest = Request<InMemoryBody>;

#[derive(Debug)]
pub struct Request<T = Body> {
    pub method: Method,
    pub url: Uri,
    pub version: Version,
    pub headers: HeaderMap,
    pub body: T,
}

impl Serialize for InMemoryRequest {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let size = 3 + if self.body.is_empty() { 0 } else { 1 };
        let mut map = serializer.serialize_map(Some(size))?;
        map.serialize_entry("method", &self.method.as_str())?;
        map.serialize_entry("url", &self.url.to_string().as_str())?;
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
            url,
            version: Default::default(),
            headers: headers.into(),
            body,
        })
    }
}

impl<T> Request<T> {
    pub fn host(&self) -> &str {
        self.url.host().unwrap_or("")
    }
    pub fn version(&self) -> Version {
        self.version
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn uri(&self) -> &Uri {
        &self.url
    }

    pub fn path(&self) -> &str {
        self.url.path()
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
}

impl Request {
    pub async fn into_memory(self) -> Result<InMemoryRequest> {
        let content_type = self.headers.get(hyper::header::CONTENT_TYPE);
        let body = self.body.into_memory(content_type).await?;
        Ok(Request {
            method: self.method,
            url: self.url,
            version: self.version,
            headers: self.headers,
            body,
        })
    }
}

impl Clone for InMemoryRequest {
    fn clone(&self) -> Self {
        Self {
            method: self.method.clone(),
            url: self.url.clone(),
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
        self.url.hash(state);
        // headers, sorted
        let mut sorted = self.headers().iter()
            .map(|(k, v)| (k.as_str(), v.as_bytes()))
            .collect::<Vec<(&str, &[u8])>>();
        sorted.sort();
        sorted.into_iter().for_each(|(k, v)| {
            k.hash(state);
            v.hash(state);
        });
        // body
        self.body.hash(state);
    }
}

impl PartialEq<Self> for Request<InMemoryBody> {
    fn eq(&self, other: &Self) -> bool {
        if !(self.method == other.method &&
            self.url == other.url &&
            self.headers == other.headers) {
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
            url: val.url,
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
            .uri(self.url);
        for (key, value) in self.headers.into_iter().filter_map(|(k, v)| Some((k?, v))) {
            builder = builder.header(key, value);
        }
        builder
            .body(self.body.into())
            .unwrap()
    }
}


#[derive(Debug)]
pub struct RequestBuilder<'a> {
    client: &'a Client,

    version: Version,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Option<Body>,
}


impl<'a> RequestBuilder<'a> {
    pub fn new(client: &'a Client, method: Method, uri: Uri) -> Self {
        RequestBuilder {
            client,
            version: Default::default(),
            method,
            uri,
            headers: Default::default(),
            body: Default::default(),
        }
    }

    pub async fn send(self) -> Result<Response> {
        let next = Next {
            client: self.client,
            middlewares: self.client.middlewares.as_slice(),
        };
        let request = self.build();
        next.run(request).await
    }

    pub fn build(self) -> Request {
        Request {
            method: self.method,
            url: self.uri,
            version: self.version,
            headers: self.headers,
            body: self.body.unwrap_or(Body::empty()),
        }
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

    pub fn push_cookie(mut self, key: &str, value: &str) -> Self {
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

    /// Overwrite the current body with the provided JSON object.
    pub fn set_json<S: Serialize>(mut self, obj: S) -> Self {
        self.body = Some(Body::InMemory(InMemoryBody::Json(serde_json::to_value(obj).unwrap())));
        if !self.headers.contains_key(&hyper::header::CONTENT_TYPE) {
            self.headers.insert(
                hyper::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
        }
        if !self.headers.contains_key(&hyper::header::ACCEPT) {
            self.headers.insert(
                hyper::header::ACCEPT,
                HeaderValue::from_static("application/json"),
            );
        }
        self
    }

    /// Add the provided JSON object to the current body.
    pub fn json<S: Serialize>(mut self, obj: S) -> Self {
        match self.body {
            None => {
                self.set_json(obj)
            }
            Some(Body::InMemory(InMemoryBody::Json(Value::Object(ref mut body)))) => {
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

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Add a url query parameter, but keep existing parameters.
    /// # Examples
    /// ```
    /// use httpclient::{Client, RequestBuilder, Method};
    /// let client = Client::new();
    /// let mut r = RequestBuilder::new(&client, Method::GET, "http://example.com/foo?a=1".parse().unwrap());
    /// r = r.query("b", "2");
    /// assert_eq!(r.uri().to_string(), "http://example.com/foo?a=1&b=2");
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

    /// Sets content-type to `application/octet-stream` and the body to the supplied bytes.
    pub fn bytes(mut self, bytes: Vec<u8>) -> Self {
        self.body = Some(Body::InMemory(InMemoryBody::Bytes(bytes)));
        self.headers.insert(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        self
    }

    /// Sets content-type to `text/plain` and the body to the supplied text.
    pub fn text(mut self, text: String) -> Self {
        self.body = Some(Body::InMemory(InMemoryBody::Text(text)));
        self.headers.insert(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );
        self
    }

    pub fn content_type(mut self, content_type: &str) -> Self {
        self.headers.insert(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_str(content_type).unwrap(),
        );
        self
    }

    /// Does not set content-type!
    pub fn set_body(mut self, body: Body) -> Self {
        self.body = Some(body);
        self
    }

    /// Normally, we have to `await` the body as well. This convenience method makes the body
    /// available immediately.
    pub fn send_awaiting_body<T: DeserializeOwned>(self) -> BoxFuture<'a, std::result::Result<Response<T>, crate::Error<InMemoryBody>>> {
        Box::pin(async move {
            let res = self.send().await;
            let res = match res {
                Ok(res) => res,
                Err(e) => return Err(e.into_memory().await),
            };
            let content_type = res.headers.get(hyper::header::CONTENT_TYPE);
            let body = res.body.into_memory(content_type).await?;
            if res.status.is_client_error() || res.status.is_server_error() {
                Err(crate::Error::HttpError(InMemoryResponse {
                    version: res.version,
                    status: res.status,
                    headers: res.headers,
                    body,
                }))
            } else {
                match body {
                    InMemoryBody::Json(value) => {
                        Ok(Response {
                            body: serde_json::from_value(value)?,
                            headers: res.headers,
                            status: res.status,
                            version: res.version,
                        })
                    }
                    _ => {
                        Err(crate::Error::JsonEncodingError(serde_json::Error::custom("Received success code, but expected JSON response.")))
                    }
                }
            }
        })
    }
}

impl<'a> IntoFuture for RequestBuilder<'a> {
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
    use hyper::header::{HeaderValue};

    use http::Method;

    use super::*;

    #[test]
    fn test_request_serialization_roundtrip() {
        #[derive(Serialize, Deserialize, Debug)]
        struct Foobar {
            a: u32,
            b: u32,
        }
        let data = Foobar { a: 1, b: 2 };
        let r1 = InMemoryRequest {
            method: Method::POST,
            url: Uri::from_str("http://example.com/").unwrap(),
            version: Default::default(),
            headers: HeaderMap::new(),
            body: InMemoryBody::Json(serde_json::to_value(&data).unwrap()),
        };
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
        let r1 = InMemoryRequest {
            method: Method::POST,
            url: Uri::from_str("http://example.com/").unwrap(),
            version: Default::default(),
            headers: HeaderMap::from_iter(vec![(
                hyper::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )].into_iter()),
            body: InMemoryBody::Json(serde_json::to_value(&data).unwrap()),
        };
        let r2 = InMemoryRequest {
            method: Method::POST,
            url: Uri::from_str("http://example.com/").unwrap(),
            version: Default::default(),
            headers: HeaderMap::from_iter(vec![(
                hyper::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )].into_iter()),
            body: InMemoryBody::Json(serde_json::to_value(&data).unwrap()),
        };
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
        let client = Client::new();
        let mut r1 = RequestBuilder::new(&client, Method::GET, "http://example.com/foo/bar".parse().unwrap());
        r1 = r1.query("a", "b");
        assert_eq!(r1.uri.to_string(), "http://example.com/foo/bar?a=b");
        r1 = r1.query("c", "d");
        assert_eq!(r1.uri.to_string(), "http://example.com/foo/bar?a=b&c=d");
    }

    #[test]
    fn test_query() {
        let client = Client::new();
        let r1 = RequestBuilder::new(&client, Method::GET, "http://example.com/foo/bar".parse().unwrap())
            .set_query(HashMap::from([("a", Some("b")), ("c", Some("d")), ("e", None)]));
        assert_eq!(r1.uri.to_string(), "http://example.com/foo/bar?a=b&c=d&e=");
    }
}