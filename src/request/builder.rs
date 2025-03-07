use std::future::IntoFuture;
use std::str::FromStr;
use std::sync::Arc;

use futures::future::BoxFuture;
use http::header::{Entry, HeaderName, ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE};
use http::uri::PathAndQuery;
use http::{header, HeaderMap, HeaderValue, Method, Uri, Version};
use serde::Serialize;
use serde_json::Value;

use crate::error::ProtocolResult;
use crate::middleware::Next;
use crate::multipart::Form;
use crate::{Client, Error, InMemoryBody, InMemoryResponse, Middleware, Request, Response};

pub static ACCEPT_JSON: HeaderValue = HeaderValue::from_static("application/json");
pub static CONTENT_JSON: HeaderValue = HeaderValue::from_static("application/json; charset=utf-8");
pub static CONTENT_URL_ENCODED: HeaderValue = HeaderValue::from_static("application/x-www-form-urlencoded");

/// Provide a custom request builder for several reasons:
/// - The required reason is have it implement IntoFuture, so that it can be directly awaited.
/// - The secondary reasons is directly storing client & middlewares on the RequestBuilder. In
///   theory it could be stored on Request.extensions, but that's less explicit.
/// - It's also nice to not require implementing an Extension trait to get all the convenience methods
///   on http::request::RequestBuilder
///
/// Middlewares are used in order (first to last).
#[derive(Debug)]
pub struct RequestBuilder<'a, C = Client, B = InMemoryBody> {
    client: &'a C,

    pub version: Version,
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: Option<B>,
    pub middlewares: Vec<Arc<dyn Middleware>>,
}

impl<'a> RequestBuilder<'a, ()> {
    pub fn get(url: &str) -> RequestBuilder<'a, ()> {
        RequestBuilder::new(&(), Method::GET, Uri::from_str(url).expect("Invalid URL"))
    }
    pub fn post(url: &str) -> RequestBuilder<'a, ()> {
        RequestBuilder::new(&(), Method::POST, Uri::from_str(url).expect("Invalid URL"))
    }
    pub fn put(url: &str) -> RequestBuilder<'a, ()> {
        RequestBuilder::new(&(), Method::PUT, Uri::from_str(url).expect("Invalid URL"))
    }
    pub fn delete(url: &str) -> RequestBuilder<'a, ()> {
        RequestBuilder::new(&(), Method::DELETE, Uri::from_str(url).expect("Invalid URL"))
    }
    pub fn head(url: &str) -> RequestBuilder<'a, ()> {
        RequestBuilder::new(&(), Method::HEAD, Uri::from_str(url).expect("Invalid URL"))
    }
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
            middlewares: Default::default(),
        }
    }

    #[must_use]
    pub fn form<S: Serialize>(mut self, obj: S) -> Self {
        match self.body {
            None => {
                let body = serde_qs::to_string(&obj).unwrap();
                self.body = Some(InMemoryBody::Text(body));
                self.headers.entry(CONTENT_TYPE).or_insert(CONTENT_URL_ENCODED.clone());
                self.headers.entry(ACCEPT).or_insert(HeaderValue::from_static("html/text"));
                self
            }
            Some(InMemoryBody::Text(ref mut body)) => {
                let new_body = serde_qs::to_string(&obj).unwrap();
                body.push('&');
                body.push_str(&new_body);
                self
            }
            _ => {
                panic!("Cannot add form to non-form body");
            }
        }
    }

    /// Overwrite the current body with the provided JSON object.
    #[must_use]
    pub fn set_json<S: Serialize>(mut self, obj: S) -> Self {
        self.body = Some(InMemoryBody::Json(serde_json::to_value(obj).unwrap()));
        self.headers.entry(CONTENT_TYPE).or_insert(CONTENT_JSON.clone());
        self.headers.entry(ACCEPT).or_insert(ACCEPT_JSON.clone());
        self
    }

    /// Add the provided JSON object to the current body.
    #[must_use]
    pub fn json<S: Serialize>(mut self, obj: S) -> Self {
        match self.body {
            None => self.set_json(obj),
            Some(InMemoryBody::Json(Value::Object(ref mut body))) => {
                if let Value::Object(obj) = serde_json::to_value(obj).unwrap() {
                    body.extend(obj);
                } else {
                    panic!("Tried to push a non-object to a json body.");
                }
                self
            }
            _ => panic!("Tried to call .json() on a non-json body. Use .set_json if you need to force a json body."),
        }
    }

    /// Sets content-type to `application/octet-stream` and the body to the supplied bytes.
    #[must_use]
    pub fn bytes(mut self, bytes: Vec<u8>) -> Self {
        // self.headers.insert(CONTENT_LENGTH, HeaderValue::from(bytes.len()));
        self.body = Some(InMemoryBody::Bytes(bytes));
        self.headers.entry(CONTENT_TYPE).or_insert(HeaderValue::from_static("application/octet-stream"));
        self
    }

    /// Sets content-type to `text/plain` and the body to the supplied text.
    #[must_use]
    pub fn text(mut self, text: String) -> Self {
        // self.headers.insert(CONTENT_LENGTH, HeaderValue::from(text.len()));
        self.body = Some(InMemoryBody::Text(text));
        self.headers.entry(CONTENT_TYPE).or_insert(HeaderValue::from_static("text/plain"));
        self
    }

    #[must_use]
    pub fn multipart<B>(mut self, form: Form<B>) -> Self
    where
        Form<B>: Into<Vec<u8>>,
    {
        let content_type = form.full_content_type();
        self.headers.entry(CONTENT_TYPE).or_insert(content_type.parse().unwrap());
        let body: Vec<u8> = form.into();
        // let len = body.len();
        match String::from_utf8(body) {
            Ok(text) => self.body = Some(InMemoryBody::Text(text)),
            Err(bytes) => self.body = Some(InMemoryBody::Bytes(bytes.into_bytes())),
        }
        // self.headers.insert(CONTENT_LENGTH, HeaderValue::from(len));
        self
    }
}

impl<'a> RequestBuilder<'a> {
    /// There are two ways to trigger the request. Immediately using `.await` will call the `IntoFuture` implementation
    /// which also awaits the body. If you want to await them separately, use this method `.send()`
    pub async fn send(self) -> ProtocolResult<Response> {
        let client = self.client;
        let (request, middlewares) = self.into_req_and_middleware();
        let next = Next {
            client,
            middlewares: &middlewares,
        };
        next.run(request).await
    }
}

impl<'a, C, B: Default> RequestBuilder<'a, C, B> {
    pub fn build(self) -> Request<B> {
        let mut b = Request::builder().method(self.method).uri(self.uri).version(self.version);
        *b.headers_mut().unwrap() = self.headers;
        b.body(self.body.unwrap_or_default()).expect("Failed to build request in .build")
    }

    pub fn into_req_and_middleware(self) -> (Request<B>, Vec<Arc<dyn Middleware>>) {
        let mut request = http::Request::builder().method(self.method).uri(self.uri).version(self.version);
        *request.headers_mut().unwrap() = self.headers;
        let request = request.body(self.body.unwrap_or_default().into()).unwrap();
        (request, self.middlewares)
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
            middlewares: Default::default(),
        }
    }

    #[must_use]
    pub fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    #[must_use]
    pub fn url(mut self, uri: &str) -> Self {
        self.uri = Uri::from_str(uri).expect("Invalid URI");
        self
    }

    #[must_use]
    pub fn set_headers<S: AsRef<str>, I: Iterator<Item = (S, S)>>(mut self, headers: I) -> Self {
        self.headers = HeaderMap::new();
        self.headers(headers)
    }

    #[must_use]
    pub fn headers<S: AsRef<str>, I: Iterator<Item = (S, S)>>(mut self, headers: I) -> Self {
        self.headers
            .extend(headers.map(|(k, v)| (HeaderName::from_str(k.as_ref()).unwrap(), HeaderValue::from_str(v.as_ref()).unwrap())));
        self
    }

    #[must_use]
    pub fn header<K: TryInto<HeaderName>>(mut self, key: K, value: &str) -> Self
    where
        <K as TryInto<HeaderName>>::Error: std::fmt::Debug,
    {
        let header = key.try_into().expect("Failed to convert key to HeaderName");
        self.headers.insert(header, HeaderValue::from_str(value).unwrap());
        self
    }

    #[must_use]
    pub fn cookie(mut self, key: &str, value: &str) -> Self {
        match self.headers.entry(COOKIE) {
            Entry::Occupied(mut e) => {
                let v = e.get_mut();
                *v = HeaderValue::from_str(&format!("{}; {}={}", v.to_str().unwrap(), key, value)).unwrap();
            }
            Entry::Vacant(_) => {
                let value = HeaderValue::from_str(&format!("{key}={value}")).unwrap();
                self.headers.insert(COOKIE, value);
            }
        }
        self
    }

    #[must_use]
    pub fn bearer_auth(mut self, token: &str) -> Self {
        self.headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}")).unwrap());
        self
    }

    #[must_use]
    pub fn token_auth(mut self, token: &str) -> Self {
        self.headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Token {token}")).unwrap());
        self
    }

    #[must_use]
    pub fn basic_auth(mut self, token: &str) -> Self {
        self.headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Basic {token}")).unwrap());
        self
    }

    /// Overwrite the query with the provided value.
    #[must_use]
    pub fn set_query<S: Serialize>(mut self, obj: S) -> Self {
        let qs = serde_qs::to_string(&obj).expect("Failed to serialize query in .set_query");
        let mut parts = std::mem::take(&mut self.uri).into_parts();
        let pq = parts.path_and_query.unwrap();
        let pq = PathAndQuery::from_str(&format!("{}?{}", pq.path(), qs)).unwrap();
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
    #[must_use]
    pub fn query(mut self, k: &str, v: &str) -> Self {
        let mut parts = std::mem::take(&mut self.uri).into_parts();
        let pq = parts.path_and_query.unwrap();
        let pq = PathAndQuery::from_str(
            match pq.query() {
                Some(q) => format!("{}?{}&{}={}", pq.path(), q, urlencoding::encode(k), urlencoding::encode(v)),
                None => format!("{}?{}={}", pq.path(), urlencoding::encode(k), urlencoding::encode(v)),
            }
            .as_str(),
        )
        .unwrap();
        parts.path_and_query = Some(pq);
        self.uri = Uri::from_parts(parts).unwrap();
        self
    }

    #[must_use]
    pub fn content_type(mut self, content_type: &str) -> Self {
        self.headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        self
    }

    /// Warning: Does not set content-type!
    #[must_use]
    pub fn body(mut self, body: B) -> Self {
        self.body = Some(body);
        self
    }

    #[must_use]
    pub fn set_middlewares(mut self, middlewares: Vec<Arc<dyn Middleware>>) -> Self {
        self.middlewares = middlewares;
        self
    }

    #[must_use]
    pub fn middleware(mut self, middleware: Arc<dyn Middleware>) -> Self {
        self.middlewares.push(middleware);
        self
    }
}

impl<'a> IntoFuture for RequestBuilder<'a, Client> {
    type Output = crate::InMemoryResult<InMemoryResponse>;
    type IntoFuture = BoxFuture<'a, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let res = self.send().await;
            let res = match res {
                Ok(res) => res,
                Err(e) => return Err(e.into()),
            };
            let (parts, body) = res.into_parts();
            let mut body = match body.into_memory().await {
                Ok(body) => body,
                Err(e) => return Err(e.into()),
            };
            if let InMemoryBody::Bytes(bytes) = body {
                body = match String::from_utf8(bytes) {
                    Ok(text) => InMemoryBody::Text(text),
                    Err(e) => InMemoryBody::Bytes(e.into_bytes()),
                };
            }
            let status = &parts.status;
            if status.is_client_error() || status.is_server_error() {
                // Prevents us from showing bytes to end users in error situations.
                Err(Error::HttpError(InMemoryResponse::from_parts(parts, body)))
            } else {
                Ok(InMemoryResponse::from_parts(parts, body))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Serialize, Deserialize)]
    pub struct TopLevel {
        inside: Nested,
    }

    #[derive(Serialize, Deserialize)]
    pub struct Nested {
        a: usize,
    }

    #[test]
    fn test_query() {
        let c = Client::new();
        let qs = TopLevel { inside: Nested { a: 1 } };
        let r = c.get("/api").set_query(qs).build();
        assert_eq!(r.uri().to_string(), "/api?inside[a]=1");
    }
}
