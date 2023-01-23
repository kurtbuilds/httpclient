

use std::str::FromStr;


use http::{HeaderMap, Version};


use hyper::{Method, Uri};




pub use builder::RequestBuilder;
pub use memory::InMemoryRequest;

use crate::{Body, Result, InMemoryBody};



mod memory;
mod builder;

#[derive(Debug)]
pub struct Request<T = Body> {
    method: Method,
    uri: Uri,
    version: Version,
    headers: HeaderMap,
    body: T,
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

    pub fn header(&self, key: &str) -> Option<&str> {
        let value = self.headers.get(key)?;
        value.to_str().ok()
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;
    use crate::Client;

    use super::*;

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