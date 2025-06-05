use std::fmt::Formatter;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};

use http::Method;
use http::Uri;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;

use crate::middleware::{Middleware, MiddlewareStack};
use crate::RequestBuilder;

static DEFAULT_HTTPS_CONNECTOR: OnceLock<HttpsConnector<HttpConnector>> = OnceLock::new();

fn default_https_connector() -> &'static HttpsConnector<HttpConnector> {
    DEFAULT_HTTPS_CONNECTOR.get_or_init(|| {
        rustls::crypto::aws_lc_rs::default_provider().install_default().expect("failed to set default provider");
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_or_http()
            .enable_http1()
            .build()
    })
}

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Clone)]
pub struct Client {
    base_url: Option<String>,
    default_headers: Vec<(String, String)>,
    pub(crate) middlewares: MiddlewareStack,
    pub(crate) inner: hyper_util::client::legacy::Client<HttpsConnector<HttpConnector>, http_body_util::Full<bytes::Bytes>>,
}

/**
what are the options?
1. `ServiceClient` provides a `OauthMiddleware`.
2. We want a way to pass in a partial middlewares list.
3. but the order is funky. we'd want something like
    - recorder
    - oauth2
    - retry
    - follow
*/
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Client {{ base_url: {:?} }}", self.base_url)
    }
}

impl Client {
    #[must_use]
    pub fn new() -> Self {
        let https = default_https_connector().clone();
        Client {
            base_url: None,
            default_headers: vec![("User-Agent".to_string(), APP_USER_AGENT.to_string())],
            middlewares: Vec::new(),
            inner: hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new()).build(https),
        }
    }

    /// Set a `base_url` so you can pass relative paths instead of full URLs.
    #[must_use]
    pub fn base_url(mut self, base_url: &str) -> Self {
        self.base_url = Some(base_url.to_string());
        self
    }

    #[must_use]
    pub fn with_middleware<T: Middleware + 'static>(mut self, middleware: T) -> Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    #[must_use]
    pub fn middleware<T: Middleware + 'static>(mut self, middleware: T) -> Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    #[must_use]
    /// Set a custom TLS connector to use for making requests.
    pub fn with_tls_connector(mut self, connector: HttpsConnector<HttpConnector>) -> Self {
        self.inner = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new()).build(connector);
        self
    }

    #[must_use]
    pub fn no_default_headers(mut self) -> Self {
        self.default_headers = Vec::new();
        self
    }

    #[must_use]
    pub fn default_headers<S: AsRef<str>, I: Iterator<Item = (S, S)>>(mut self, headers: I) -> Self {
        self.default_headers.extend(headers.map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string())));
        self
    }

    #[must_use]
    pub fn default_header<S: AsRef<str>>(mut self, key: S, value: S) -> Self {
        self.default_headers.push((key.as_ref().to_string(), value.as_ref().to_string()));
        self
    }

    #[must_use]
    fn build_uri(&self, uri_or_path: &str) -> Uri {
        if let Ok(uri) = Uri::from_str(uri_or_path) {
            if uri.scheme().is_some() && uri.host().is_some() {
                return uri;
            }
        }
        let uri = self.base_url.as_ref().map_or_else(|| uri_or_path.to_string(), |s| s.clone() + uri_or_path);
        Uri::from_str(&uri).unwrap()
    }

    #[must_use]
    pub fn get(&self, url_or_path: impl AsRef<str>) -> RequestBuilder<Client> {
        self.request(Method::GET, url_or_path.as_ref())
    }

    #[must_use]
    pub fn post(&self, uri_or_path: impl AsRef<str>) -> RequestBuilder<Client> {
        self.request(Method::POST, uri_or_path.as_ref())
    }

    #[must_use]
    pub fn delete(&self, uri_or_path: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::DELETE, uri_or_path.as_ref())
    }

    #[must_use]
    pub fn put(&self, uri_or_path: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::PUT, uri_or_path.as_ref())
    }

    #[must_use]
    pub fn patch(&self, uri_or_path: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::PATCH, uri_or_path.as_ref())
    }

    #[must_use]
    pub fn request(&self, method: Method, uri_or_path: impl AsRef<str>) -> RequestBuilder {
        let uri = self.build_uri(uri_or_path.as_ref());
        RequestBuilder::new(self, method, uri)
            .headers(self.default_headers.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .set_middlewares(self.middlewares.clone())
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::middleware::{Recorder, RecorderMode};
    use crate::ResponseExt;

    use super::*;

    #[tokio::test]
    async fn test_make_request() {
        let client = Client::new()
            .base_url("https://www.jsonip.com")
            .no_default_headers()
            .default_headers(vec![("User-Agent", "test-client")].into_iter())
            .with_middleware(Recorder::new().mode(RecorderMode::ForceNoRequests));

        let res = client.get("/").send().await.unwrap().json::<HashMap<String, String>>().await.unwrap();
        let res = serde_json::to_value(res).unwrap();
        assert_eq!(
            res,
            serde_json::json!({"ip":"70.107.97.117","geo-ip":"https://getjsonip.com/#plus","API Help":"https://getjsonip.com/#docs"})
        );
    }
}
