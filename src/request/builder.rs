use std::future::IntoFuture;
use std::str::FromStr;
use std::sync::Arc;

use futures::future::BoxFuture;
use http::header::{Entry, HeaderName};
use http::uri::PathAndQuery;
use http::{HeaderMap, HeaderValue, Method, Uri, Version};
use hyper::header;
use serde::Serialize;
use serde_json::Value;

use crate::error::ProtocolResult;
use crate::middleware::Next;
use crate::multipart::Form;
use crate::{Client, Error, InMemoryBody, InMemoryResponse, Middleware, Request, Response};

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
                self.body = Some(InMemoryBody::Text(serde_qs::to_string(&obj).unwrap()));
                self.headers
                    .entry(header::CONTENT_TYPE)
                    .or_insert(HeaderValue::from_static("application/x-www-form-urlencoded"));
                self.headers.entry(header::ACCEPT).or_insert(HeaderValue::from_static("html/text"));
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
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        self.headers.entry(header::ACCEPT).or_insert(HeaderValue::from_static("application/json"));
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
        self.body = Some(InMemoryBody::Bytes(bytes));
        self.headers.entry(header::CONTENT_TYPE).or_insert(HeaderValue::from_static("application/octet-stream"));
        self
    }

    /// Sets content-type to `text/plain` and the body to the supplied text.
    #[must_use]
    pub fn text(mut self, text: String) -> Self {
        self.body = Some(InMemoryBody::Text(text));
        self.headers.entry(header::CONTENT_TYPE).or_insert(HeaderValue::from_static("text/plain"));
        self
    }

    #[must_use]
    pub fn multipart(mut self, form: Form) -> Self {
        self.headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_str(form.content_type.as_str()).unwrap());
        let body: Vec<u8> = form.into();
        let len = body.len();
        self.body = Some(InMemoryBody::Bytes(body));
        self.headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
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
        Request {
            method: self.method,
            uri: self.uri,
            version: self.version,
            headers: self.headers,
            body: self.body.unwrap_or_default(),
        }
    }

    pub fn into_req_and_middleware(self) -> (Request<B>, Vec<Arc<dyn Middleware>>) {
        (
            Request {
                method: self.method,
                uri: self.uri,
                version: self.version,
                headers: self.headers,
                body: self.body.unwrap_or_default(),
            },
            self.middlewares,
        )
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
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(HeaderName::from_str(key).unwrap(), HeaderValue::from_str(value).unwrap());
        self
    }

    #[must_use]
    pub fn cookie(mut self, key: &str, value: &str) -> Self {
        match self.headers.entry(hyper::header::COOKIE) {
            Entry::Occupied(mut e) => {
                let v = e.get_mut();
                *v = HeaderValue::from_str(&format!("{}; {}={}", v.to_str().unwrap(), key, value)).unwrap();
            }
            Entry::Vacant(_) => {
                self.headers.insert(hyper::header::COOKIE, HeaderValue::from_str(&format!("{key}={value}")).unwrap());
            }
        }
        self
    }

    #[must_use]
    pub fn bearer_auth(mut self, token: &str) -> Self {
        self.headers.insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}")).unwrap());
        self
    }

    #[must_use]
    pub fn token_auth(mut self, token: &str) -> Self {
        self.headers.insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Token {token}")).unwrap());
        self
    }

    #[must_use]
    pub fn basic_auth(mut self, token: &str) -> Self {
        self.headers.insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Basic {token}")).unwrap());
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
            let status = &parts.status;
            if status.is_client_error() || status.is_server_error() {
                // Prevents us from showing bytes to end users in error situations.
                if let InMemoryBody::Bytes(bytes) = body {
                    body = match String::from_utf8(bytes) {
                        Ok(text) => InMemoryBody::Text(text),
                        Err(e) => InMemoryBody::Bytes(e.into_bytes()),
                    };
                }
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
