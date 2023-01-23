use http::{HeaderMap, HeaderValue, Method, Uri, Version};
use http::header::HeaderName;
use serde::Serialize;
use serde_json::Value;
use std::borrow::Cow;
use http::uri::PathAndQuery;
use futures::future::BoxFuture;
use std::future::IntoFuture;
use std::str::FromStr;
use crate::{Body, Client, InMemoryBody, Request, Response, InMemoryResponse};
use crate::middleware::Next;

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
    pub async fn send(self) -> crate::Result<Response> {
        let next = Next {
            client: self.client,
            middlewares: self.client.middlewares.as_slice(),
        };
        let request = self.build();
        next.run(request.into()).await
    }

    /// Normally, we have to `await` the body as well. This convenience method makes the body
    /// available immediately.
    pub fn send_awaiting_body(self) -> BoxFuture<'a, crate::Result<InMemoryResponse, crate::Error<InMemoryBody>>> {
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

    pub fn try_build(self) -> crate::Result<Request<B>, crate::Error> {
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
    type Output = crate::Result<Response>;
    type IntoFuture = BoxFuture<'a, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.send())
    }
}
