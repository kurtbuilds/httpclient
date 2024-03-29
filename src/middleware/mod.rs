use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use cookie::time;
use cookie::time::format_description::well_known::Rfc2822;
use http::Uri;
use tokio::time::Duration;

pub use recorder::*;

use crate::{Body, InMemoryBody, InMemoryRequest, Response};
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
            let request = request.into_hyper();
            let res = self.client.inner.request(request).await?;
            let (parts, body) = res.into_parts();
            let body: Body = body.into();
            let res = Response::from_parts(parts, body);
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
/// Retry a request up to 3 times.
/// TODO: Make this configurable.
/// TODO: Delays
/// TODO: Backoff
pub struct Retry;

fn calc_delay(res: &Response) -> Option<Duration> {
    let Some(v) = res.headers().get(http::header::RETRY_AFTER) else { return None; };
    let retry_after = v.to_str().unwrap();
    if let Some(retry_after) = retry_after.parse().ok() {
        Some(Duration::from_secs(retry_after))
    } else if let Some(dt) = time::OffsetDateTime::parse(retry_after, &Rfc2822).ok() {
        let dur = dt - time::OffsetDateTime::now_utc();
        Some(dur.try_into().unwrap())
    } else {
        None
    }
}

#[async_trait]
impl Middleware for Retry {
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let mut i = 0usize;
        loop {
            i += 1;
            if i > 3 {
                return Err(ProtocolError::TooManyRetries)
            }
            match next.run(request.clone().into()).await {
                Ok(res) => {
                    let status = res.status();
                    let status_as_u16 = status.as_u16();
                    if !([429, 408, 425].contains(&status_as_u16) || status.is_server_error()) {
                        return Ok(res);
                    }
                    if let Some(delay) = calc_delay(&res) {
                        tokio::time::sleep(delay).await;
                    }
                }
                Err(err) => return Err(err),
            }
        }
    }
}

#[derive(Debug)]
pub struct Logger;

fn headers_to_string(headers: &http::HeaderMap, dir: char) -> String {
    headers
        .iter()
        .map(|(k, v)| format!("{dir} {}: {}", k, v.to_str().unwrap()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[async_trait]
impl Middleware for Logger {
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let url = request.uri().to_string();
        let method = request.method().as_str().to_uppercase();
        let version = request.version();
        let headers = headers_to_string(request.headers(), '>');
        let body = request.body();
        println!(">>> Request:
> {method} {url} {version:?}
{headers}");
        if !body.is_empty() {
            match body {
                InMemoryBody::Text(s) => println!("{}", s),
                InMemoryBody::Json(o) => println!("{}", serde_json::to_string(&o).unwrap()),
                _ => println!("{:?}", body),
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
                println!("<<< Response to {url}:
< {version:?} {status}
{headers}");
                let (parts, body) = res.into_parts();
                let content_type = parts.headers.get(http::header::CONTENT_TYPE);
                let body = body.into_content_type(content_type).await?;
                match &body {
                    InMemoryBody::Text(text) => println!("{}", text),
                    InMemoryBody::Json(o) => println!("{}", serde_json::to_string(&o).unwrap()),
                    _ => println!("{:?}", body),
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
        let mut res = next.run(request.clone().into()).await?;
        let mut allowed_redirects = 10;
        while res.status().is_redirection() {
            if allowed_redirects == 0 {
                return Err(ProtocolError::TooManyRedirects);
            }
            let redirect = res.headers().get(http::header::LOCATION).expect("Received a 3xx status code, but no location header was sent.").to_str().unwrap();
            let url = fix_url(request.url(), redirect);
            let request = request.clone();
            let request = request.set_url(url);
            allowed_redirects -= 1;
            res = next.run(request.into()).await?;
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