use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use cookie::time;
use cookie::time::format_description::well_known::Rfc2822;
use http::header::{CONTENT_LENGTH, LOCATION};
use hyper::body::Bytes;
use tokio::time::Duration;

pub use recorder::*;

use crate::{Body, InMemoryBody, InMemoryRequest, Response, Uri};
use crate::client::Client;
use crate::error::{ProtocolError, ProtocolResult};

mod recorder;

pub type MiddlewareStack = Vec<Arc<dyn Middleware>>;

#[derive(Debug, Copy, Clone)]
pub struct Next<'a> {
    pub client: &'a Client,
    pub(crate) middlewares: &'a [Arc<dyn Middleware>],
}

impl Next<'_> {
    pub async fn run(self, request: InMemoryRequest) -> ProtocolResult<Response> {
        if let Some((middleware, rest)) = self.middlewares.split_first() {
            let next = Next {
                client: self.client,
                middlewares: rest,
            };
            middleware.handle(request, next).await
        } else {
            let (mut parts, body) = request.into_parts();
            let body = match body {
                InMemoryBody::Empty => Bytes::new(),
                InMemoryBody::Bytes(b) => Bytes::from(b),
                InMemoryBody::Text(s) => Bytes::from(s),
                InMemoryBody::Json(val) => Bytes::from(serde_json::to_string(&val)?),
            };
            let len = body.len();
            parts.headers.insert(CONTENT_LENGTH, len.into());
            let mut b = hyper::Request::builder()
                .method(parts.method.as_str())
                .uri(parts.uri.to_string());
            for (k, v) in parts.headers.iter() {
                b = b.header(k.as_str(), v.to_str().unwrap());
            }
            let request = b.body(hyper::Body::from(body))
                .expect("Failed to build request");
            let res = self.client.inner.request(request).await?;
            let (parts, body) = res.into_parts();
            let body: Body = body.into();
            let mut b = Response::builder()
                .status(parts.status.as_u16());
            for (k, v) in parts.headers.iter() {
                b = b.header(k.as_str(), v.to_str().unwrap());
            }
            let res = b.body(body)
                .expect("Failed to build response");
            Ok(res)
        }
    }
}

#[async_trait]
pub trait Middleware: Send + Sync + Debug {
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        next.run(request).await
    }
}

#[derive(Debug)]
/// Retry a request up to N times, with a default of 3.
/// The default back-off delay is 2 seconds.
pub struct Retry {
    max_retries: usize,
    backoff_delay: Duration,
}

fn calc_delay(res: &Response) -> Option<Duration> {
    let v = res.headers().get(http::header::RETRY_AFTER)?;
    let retry_after = v.to_str().unwrap();

    if let Ok(retry_after) = retry_after.parse() {
        Some(Duration::from_secs(retry_after))
    } else if let Ok(dt) = time::OffsetDateTime::parse(retry_after, &Rfc2822) {
        let dur = dt - time::OffsetDateTime::now_utc();
        Some(dur.try_into().unwrap())
    } else {
        None
    }
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            backoff_delay: Duration::from_secs(2),
            max_retries: 3,
        }
    }
}

impl Retry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the back-off delay between retries, if the server doesn't specify a delay.
    pub fn backoff_delay(mut self, delay: Duration) -> Self {
        self.backoff_delay = delay;
        self
    }

    /// Set the maximum number of retries.
    pub fn max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }
}

#[async_trait]
impl Middleware for Retry {
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let mut i = 0usize;
        let mut delay = Duration::from_millis(100); // Initial delay

        loop {
            i += 1;
            if i > self.max_retries {
                return Err(ProtocolError::TooManyRetries);
            }
            match next.run(request.clone()).await {
                Ok(res) => {
                    let status = res.status();
                    let status_as_u16 = status.as_u16();

                    // Can't use StatusCode here, as it doesn't implement 425/TOO_EARLY
                    if !([429, 408, 425].contains(&status_as_u16) || status.is_server_error()) {
                        return Ok(res);
                    }

                    if let Some(custom_delay) = calc_delay(&res) {
                        delay = custom_delay;
                    } else {
                        delay *= 2; // Exponential back-off
                    }

                    tokio::time::sleep(delay).await;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

#[derive(Debug)]
pub struct Logger;

fn headers_to_string(headers: &http::HeaderMap, dir: char) -> String {
    headers.iter().map(|(k, v)| format!("{dir} {}: {}", k, v.to_str().unwrap())).collect::<Vec<_>>().join("\n")
}

#[async_trait]
impl Middleware for Logger {
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let url = request.uri().to_string();
        let method = request.method().as_str().to_uppercase();
        let version = request.version();
        let headers = headers_to_string(request.headers(), '>');
        let body = request.body();
        println!(
            ">>> Request:
> {method} {url} {version:?}
{headers}"
        );
        if !body.is_empty() {
            match body {
                InMemoryBody::Text(s) => println!("{s}"),
                InMemoryBody::Json(o) => println!("{}", serde_json::to_string(&o).unwrap()),
                _ => println!("{body:?}"),
            }
        }
        let res = next.run(request).await;
        match res {
            Err(e) => {
                println!("<<< Response to {url}:\n{e}");
                Err(e)
            }
            Ok(res) => {
                let version = res.version();
                let status = res.status();
                let headers = headers_to_string(res.headers(), '<');
                println!(
                    "<<< Response to {url}:
< {version:?} {status}
{headers}"
                );
                let (parts, body) = res.into_parts();
                let content_type = parts.headers.get(http::header::CONTENT_TYPE);
                let body = body.into_content_type(content_type).await?;
                match &body {
                    InMemoryBody::Text(text) => println!("{text}"),
                    InMemoryBody::Json(o) => println!("{}", serde_json::to_string(&o).unwrap()),
                    _ => println!("{body:?}"),
                }
                let res = Response::from_parts(parts, body.into());
                Ok(res)
            }
        }
    }
}

#[derive(Debug, Clone)]
/// Follow redirects.
pub struct Follow;

/// Given an original Url, redirect to the new path.
fn fix_url(original: &Uri, redirect_url: &str) -> Uri {
    let url = Uri::from_str(redirect_url).unwrap();
    let mut parts = url.into_parts();
    if parts.authority.is_none() {
        parts.authority = original.authority().cloned();
    }
    if parts.scheme.is_none() {
        parts.scheme = original.scheme().cloned();
    }
    Uri::from_parts(parts).unwrap()
}

#[async_trait]
impl Middleware for Follow {
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let mut res = next.run(request.clone()).await?;
        let mut allowed_redirects = 10;
        while res.status().is_redirection() {
            if allowed_redirects == 0 {
                return Err(ProtocolError::TooManyRedirects);
            }
            let redirect = res
                .headers()
                .get(LOCATION)
                .expect("Received a 3xx status code, but no location header was sent.")
                .to_str()
                .unwrap();
            let url = fix_url(request.uri(), redirect);
            let mut request: InMemoryRequest = request.clone();
            *request.uri_mut() = url;
            allowed_redirects -= 1;
            res = next.run(request).await?;
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_route() {
        let original = Uri::from_str("https://www.google.com/").unwrap();
        let url = fix_url(&original, "/test");
        assert_eq!(url.to_string(), "https://www.google.com/test");
    }
}
