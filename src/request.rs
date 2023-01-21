use futures::future::BoxFuture;
use std::borrow::Cow;
use std::fmt;
use std::future::IntoFuture;

use std::str::FromStr;

use http::header::HeaderName;
use http::request::Parts;
use http::{HeaderValue, Version};
use http::uri::PathAndQuery;
use hyper::{Method, Uri};
use crate::client::Client;
use crate::response::{Response, ResponseWithBody};
use crate::error;
use crate::body::{Body, NonStreamingBody};
use serde::{Serialize, Deserialize, Deserializer};
use serde::de::{DeserializeOwned, MapAccess};
use serde::ser::SerializeMap;
use serde_json::Value;
use crate::headers::{AddHeaders, SortedHeaders};


#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Request(
    #[serde(serialize_with = "serialize_request")]
    hyper::Request<Body>
);

impl Request {
    pub fn version(&self) -> Version {
        self.0.version()
    }

    pub fn method(&self) -> &Method {
        self.0.method()
    }

    pub fn url(&self) -> &Uri {
        self.0.uri()
    }

    pub fn host(&self) -> &str {
        self.url().host().unwrap_or("")
    }

    pub fn path(&self) -> &str {
        self.url().path()
    }

    pub fn body(&self) -> &Body {
        self.0.body()
    }

    pub fn body_mut(&mut self) -> &mut Body {
        self.0.body_mut()
    }

    pub fn headers(&self) -> &hyper::HeaderMap {
        self.0.headers()
    }

    pub fn headers_mut(&mut self) -> &hyper::HeaderMap {
        self.0.headers_mut()
    }

    pub fn into_parts(self) -> (http::request::Parts, Body) {
        self.0.into_parts()
    }

    pub fn into_inner(self) -> hyper::Request<hyper::Body> {
        let (parts, body) = self.into_parts();
        let body: hyper::Body = body.into();
        hyper::Request::from_parts(parts, body)
    }

    pub fn from_parts(parts: Parts, body: Body) -> Self {
        Request(hyper::Request::from_parts(parts, body))
    }

    pub fn try_clone(&self) -> Result<Self, crate::Error> {
        let builder = hyper::Request::builder()
            .version(self.version())
            .method(self.method().clone())
            .headers(self.headers())
            .uri(self.url().clone());
        Ok(Request(builder
            .body(self.body().try_clone()?)
            .unwrap()))
    }

    pub(crate) async fn into_infallible_cloneable(self) -> Result<Self, crate::Error> {
        let (parts, body) = self.into_parts();
        let content_type = parts.headers.get(hyper::header::CONTENT_TYPE);
        let body = body.into_memory(content_type).await?;
        Ok(Request::from_parts(parts, body))
    }
}


impl From<hyper::Request<hyper::Body>> for Request {
    fn from(request: hyper::Request<hyper::Body>) -> Self {
        let (parts, body) = request.into_parts();
        let body: Body = body.into();
        Request(hyper::Request::from_parts(parts, body))
    }
}


impl std::hash::Hash for Request {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.method().hash(state);
        self.url().hash(state);
        let mut sorted = self.headers().iter()
            .map(|(k, v)| (k.as_str(), v.as_bytes()))
            .collect::<Vec<(&str, &[u8])>>();
        sorted.sort();
        sorted.into_iter().for_each(|(k, v)| {
            k.hash(state);
            v.hash(state);
        });
        self.body().hash(state);
    }
}

impl PartialEq<Self> for Request {
    fn eq(&self, other: &Self) -> bool {
        if !(self.method() == other.method() &&
            self.url() == other.url() &&
            self.headers() == other.headers()) {
            return false;
        }
        match (self.body(), other.body()) {
            (Body::Empty, Body::Empty) => true,
            (Body::Text(ref a), Body::Text(ref b)) => a == b,
            (Body::Bytes(ref a), Body::Bytes(ref b)) => a == b,
            (Body::Json(ref a), Body::Json(ref b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Request {}


fn serialize_request<S>(value: &hyper::Request<Body>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
{
    let _size = 3 + if value.body().is_empty() { 0 } else { 1 };
    let mut map = serializer.serialize_map(Some(4))?;
    map.serialize_entry("method", value.method().as_str())?;
    map.serialize_entry("url", value.uri().to_string().as_str())?;
    map.serialize_entry("headers", &SortedHeaders::from(value.headers()))?;
    if !value.body().is_empty() {
        map.serialize_entry("data", &NonStreamingBody::from(value.body()))?;
    }
    map.end()
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
    {
        deserializer.deserialize_map(RequestVisitor)
    }
}

struct RequestVisitor;


impl<'de> serde::de::Visitor<'de> for RequestVisitor {
    type Value = Request;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("A map with the following keys: method, url, headers, data")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error> where A: MapAccess<'de> {
        let mut method = None;
        let mut url = None;
        let mut headers = None;
        let mut data = None;
        while let Some(key) = map.next_key::<Cow<str>>()? {
            match key.as_ref() {
                "method" => {
                    if method.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("method"));
                    }
                    let s = map.next_value::<String>()?;
                    method = Some(Method::from_str(&s).map_err(|_e|
                        <A::Error as serde::de::Error>::custom("Invalid value for field `method`.")
                    )?);
                }
                "url" => {
                    if url.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("url"));
                    }
                    let s = map.next_value::<String>()?;
                    url = Some(Uri::from_str(&s).map_err(|_e|
                        <A::Error as serde::de::Error>::custom("Invalid value for field `url`.")
                    )?);
                }
                "data" => {
                    if data.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("data"));
                    }
                    data = Some(map.next_value::<NonStreamingBody>()?);
                }
                "headers" => {
                    if headers.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("headers"));
                    }
                    let s = map.next_value::<SortedHeaders>()?;
                    headers = Some(s);
                }
                _ => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        let method = method.ok_or_else(|| serde::de::Error::missing_field("method"))?;
        let url = url.ok_or_else(|| serde::de::Error::missing_field("url"))?;
        let headers = headers.ok_or_else(|| serde::de::Error::missing_field("headers"))?;
        let data = data.unwrap_or(NonStreamingBody::Empty);
        Ok(Request(hyper::Request::builder()
            .method(method)
            .uri(url)
            .headers_from_sorted(headers)
            .body(data.into())
            .unwrap()))
    }
}


#[derive(Debug)]
pub struct RequestBuilder<'a> {
    client: &'a Client,

    version: hyper::Version,
    method: Method,
    uri: Uri,
    headers: hyper::HeaderMap,
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

    pub async fn send(self) -> Result<Response, error::Error> {
        self.client.execute(self).await
    }

    pub fn build(self) -> Request {
        let b = hyper::Request::builder()
            .method(&self.method)
            .uri(&self.uri)
            .version(self.version)
            .headers(&self.headers);
        Request(b.body(self.body.unwrap_or(Body::Empty)).unwrap())
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
    pub fn json<S: Serialize>(mut self, obj: S) -> Self {
        self.body = Some(Body::Json(serde_json::to_value(obj).unwrap()));
        self.headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        self
    }

    pub fn push_json<S: Serialize>(mut self, obj: S) -> Self {
        match self.body {
            None => {
                self.json(obj)
            }
            Some(Body::Json(serde_json::Value::Object(ref mut body))) => {
                if let Value::Object(obj) = serde_json::to_value(obj).unwrap() {
                    body.extend(obj.into_iter());
                } else {
                    panic!("Invalid json object");
                }
                self
            }
            _ => panic!("Invalid json object"),
        }
    }

    /// Destructively sets the query. If any query params are already set, they will be overwritten.
    pub fn query<S: Serialize>(mut self, obj: S) -> Self {
        let query = {
            let val = serde_json::to_value(obj).unwrap();
            let map = val.as_object().unwrap();
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
    /// r = r.push_query("b", "2");
    /// assert_eq!(r.uri().to_string(), "http://example.com/foo?a=1&b=2");
    /// ```
    pub fn push_query(mut self, k: &str, v: &str) -> Self {
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
    pub fn bytes(mut self, bytes: &[u8]) -> Self {
        self.body = Some(Body::Bytes(bytes.to_vec()));
        self.headers.insert(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        self
    }

    /// Sets content-type to `text/plain` and the body to the supplied text.
    pub fn text(mut self, text: &str) -> Self {
        self.body = Some(Body::Text(text.to_string()));
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
}

pub trait SendFull<'a, T> {
    fn send_full(self) -> BoxFuture<'a, Result<ResponseWithBody<T>, crate::Error>>;
}

// impl<'a> SendFull<'a, String> for RequestBuilder<'a> {
//     fn send_full(self) -> BoxFuture<'a, Result<ResponseWithBody<String>, crate::Error>> {
//         Box::pin(async move {
//             let res = self.send().await?;
//             let (parts, body) = res.into_parts();
//             let Body::Hyper(hyper_body) = body else {
//                 return Err(crate::Error::Generic("Invalid body".to_string()));
//             };
//             let bytes = hyper::body::to_bytes(hyper_body).await?;
//             let body = String::from_utf8(bytes.to_vec())?;
//             if parts.status.is_client_error() || parts.status.is_server_error() {
//                 Err(crate::Error::ApplicationErrorText {
//                     status: parts.status,
//                     headers: parts.headers,
//                     body,
//                 })
//             } else {
//                 Ok(ResponseWithBody {
//                     data: body,
//                     headers: parts.headers,
//                     status: parts.status,
//                 })
//             }
//         })
//     }
// }

impl<'a, T> SendFull<'a, T> for RequestBuilder<'a>
    where
        T: DeserializeOwned + 'a,
{
    fn send_full(self) -> BoxFuture<'a, Result<ResponseWithBody<T>, crate::Error>> {
        Box::pin(async move {
            let res = self.send().await?;
            let (parts, body) = res.into_parts();
            let Body::Hyper(hyper_body) = body else {
                return Err(crate::Error::Generic("Invalid body".to_string()));
            };
            let bytes = hyper::body::to_bytes(hyper_body).await?;
            if parts.status.is_client_error() || parts.status.is_server_error() {
                match serde_json::from_slice(&bytes) {
                    Ok(v) => {
                        Err(crate::Error::ApplicationErrorJson {
                            status: parts.status,
                            headers: parts.headers,
                            body: v,
                        })
                    },
                    Err(_) => {
                        let body = String::from_utf8(bytes.to_vec())?;
                        Err(crate::Error::ApplicationErrorText {
                            status: parts.status,
                            headers: parts.headers,
                            body,
                        })
                    }
                }
            } else {
                let body = T::deserialize(&mut serde_json::Deserializer::from_slice(&bytes))?;
                Ok(ResponseWithBody {
                    data: body,
                    headers: parts.headers,
                    status: parts.status,
                })
            }
        })
    }
}

impl<'a> IntoFuture for RequestBuilder<'a> {
    type Output = Result<Response, crate::Error>;
    type IntoFuture = BoxFuture<'a, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.client.execute(self))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use super::*;
    use http::Method;

    #[test]
    fn test_request_serialization_roundtrip() {
        #[derive(Serialize, Deserialize, Debug)]
        struct Foobar {
            a: u32,
            b: u32,
        }
        let data = Foobar { a: 1, b: 2 };
        let r1 = Request(hyper::Request::builder()
            .method(Method::POST)
            .header("content-type", "application/json")
            .uri("http://example.com/")
            .body(Body::Text(serde_json::to_string(&data).unwrap())).unwrap());
        let s = serde_json::to_string_pretty(&r1).unwrap();
        let r2: Request = serde_json::from_str(&s).unwrap();
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
        let r1 = Request(hyper::Request::builder()
            .method(Method::POST)
            .header("content-type", "application/json")
            .header("user-agent", "httpclient/0.1.0")
            .uri("http://example.com/")
            .body(Body::Json(serde_json::to_value(&data).unwrap())).unwrap());
        let r2 = Request(hyper::Request::builder()
            .method(Method::POST)
            .header("user-agent", "httpclient/0.1.0")
            .header("content-type", "application/json")
            .uri("http://example.com/")
            .body(Body::Json(serde_json::to_value(&data).unwrap())).unwrap());
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
        r1 = r1.push_query("a", "b");
        assert_eq!(r1.uri.to_string(), "http://example.com/foo/bar?a=b");
        r1 = r1.push_query("c", "d");
        assert_eq!(r1.uri.to_string(), "http://example.com/foo/bar?a=b&c=d");
    }

    #[test]
    fn test_query() {
        let client = Client::new();
        let r1 = RequestBuilder::new(&client, Method::GET, "http://example.com/foo/bar".parse().unwrap())
            .query(HashMap::from([("a", Some("b")), ("c", Some("d")), ("e", None)]));
        assert_eq!(r1.uri.to_string(), "http://example.com/foo/bar?a=b&c=d&e=");
    }
}