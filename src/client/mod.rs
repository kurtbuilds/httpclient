use std::fmt::Formatter;
use std::str::FromStr;
use http::Method;
use hyper::client::HttpConnector;
use hyper::{Uri};
use hyper_rustls::HttpsConnector;
use crate::request::RequestBuilder;
use crate::middleware::Middleware;
use crate::response::Response;
use crate::middleware::Next;
use crate::{Error, Request};


static APP_USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
);

pub struct Client {
    base_url: Option<String>,
    default_headers: Vec<(String, String)>,
    pub(crate) middlewares: Vec<Box<dyn Middleware>>,
    inner: hyper::Client<HttpsConnector<HttpConnector>, hyper::Body>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Client {{ base_url: {:?} }}", self.base_url)
    }
}


impl Client {
    pub fn new(mut base_url: Option<String>) -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .build();
        base_url.as_mut().map(|url| {
            if url.ends_with('/') {
                url.truncate(url.len() - 1);
            }
        });
        Client {
            base_url,
            default_headers: vec![("User-Agent".to_string(), APP_USER_AGENT.to_string())],
            middlewares: Vec::new(),
            inner: hyper::Client::builder().build(https),
        }
    }

    pub fn with_middleware<T: Middleware + 'static>(mut self, middleware: T) -> Self {
        self.middlewares.push(Box::new(middleware));
        self
    }

    fn build_uri(&self, uri_or_path: &str) -> Uri {
        let uri = self.base_url.as_ref().map(|s| s.clone() + uri_or_path).unwrap_or(uri_or_path.to_string());
        Uri::from_str(&uri).unwrap()
    }

    pub fn get(&self, url_or_path: &str) -> RequestBuilder {
        let uri = self.build_uri(url_or_path);
        RequestBuilder::new(self, Method::GET, uri)
            .headers(self.default_headers.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    }

    pub fn post(&self, uri_or_path: &str) -> RequestBuilder {
        let uri = self.build_uri(uri_or_path);
        RequestBuilder::new(self, Method::POST, uri)
            .headers(self.default_headers.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    }

    pub fn request(&self, method: Method, uri_or_path: &str) -> RequestBuilder {
        let uri = self.build_uri(uri_or_path);
        RequestBuilder::new(self, method, uri)
            .headers(self.default_headers.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    }

    pub async fn execute(&self, builder: RequestBuilder<'_>) -> Result<Response, Error> {
        let next = Next {
            client: self,
            middlewares: self.middlewares.as_slice(),
        };
        let request = builder.build();

        next.run(request).await
    }

    pub fn default_headers<S: AsRef<str>, I: Iterator<Item=(S, S)>>(mut self, headers: I) -> Self {
        self.default_headers.extend(headers.map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()) ));
        self
    }

    pub fn default_header<S: AsRef<str>>(mut self, key: S, value: S) -> Self {
        self.default_headers.push((key.as_ref().to_string(), value.as_ref().to_string()));
        self
    }

    /// This is the internal method to actually send the request. It assumes that middlewares have already been executed.
    /// `execute` is the pub method that additionally runs middlewares.
    pub async fn send(&self, request: Request) -> Result<Response, Error> {
        Ok(Response::from(self.inner.request(request.into_inner()).await?))
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    
    use crate::middleware::{RecorderMiddleware, RecorderMode};
    use super::*;

    #[tokio::test]
    async fn test_make_request() {
        let client = Client::new(Some("https://www.jsonip.com".to_string()))
            .with_middleware(RecorderMiddleware::with_mode(RecorderMode::RecordOrRequest));

        let res = serde_json::to_value(client.get("/")
            .send()
            .await
            .unwrap()
            .json::<HashMap<String, String>>()
            .await
            .unwrap()).unwrap();
        assert_eq!(res, serde_json::json!({"ip":"70.107.72.13","geo-ip":"https://getjsonip.com/#plus","API Help":"https://getjsonip.com/#docs"}));
    }
}