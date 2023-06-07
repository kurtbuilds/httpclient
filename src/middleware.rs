use std::str::FromStr;
use crate::{Error, Response};

use async_trait::async_trait;
use http::Uri;
use crate::client::Client;
use crate::error::ProtocolError;
use crate::request::{Request};
use crate::recorder::RequestRecorder;
use tracing::info;

#[derive(Copy, Clone)]
pub struct Next<'a> {
    pub(crate) client: &'a Client,
    pub(crate) middlewares: &'a [Box<dyn Middleware>],
}

impl Next<'_> {
    pub async fn run(self, request: Request) -> Result<Response, Error> {
        if let Some((middleware, rest)) = self.middlewares.split_first() {
            let next = Next {
                client: self.client,
                middlewares: rest,
            };
            middleware.handle(request, next).await
        } else {
            self.client.send_request(request).await
        }
    }
}

#[async_trait]
pub trait Middleware: Send + Sync {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        next.run(request).await
    }
}

/// This middleware caches requests to the local filesystem. Subsequent requests will return results
/// from the filesystem, and not touch the remote server.
///
/// The recordings are sanitized to hide secrets.
///
/// Use `.mode()` to configure the behavior:
/// - `RecorderMode::RecordOrRequest` (default): Will check for recordings, but will make the request if no recording is found.
/// - `RecorderMode::IgnoreRecordings`: Always make the request. (Use to force refresh recordings.)
/// - `RecorderMode::ForceNoRequests`: Fail if no recording is found. (Use to run tests without hitting the remote server.)
pub struct RecorderMiddleware {
    mode: RecorderMode,
    pub request_recorder: RequestRecorder,
}

impl Default for RecorderMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl RecorderMiddleware {
    pub fn new() -> Self {
        Self {
            mode: RecorderMode::RecordOrRequest,
            request_recorder: RequestRecorder::new(),
        }
    }

    pub fn mode(self, mode: RecorderMode) -> Self {
        Self {
            mode,
            request_recorder: self.request_recorder,
        }
    }

    fn should_lookup(&self) -> bool {
        self.mode.should_lookup()
    }

    fn should_request(&self) -> bool {
        self.mode.should_request()
    }
}


#[derive(PartialEq, Eq, Clone, Copy)]
pub enum RecorderMode {
    /// Default. Will check for recordings, but will make the request if no recording is found.
    RecordOrRequest,
    /// Always make the request.
    IgnoreRecordings,
    /// Always use recordings. Fail if no recording is found.
    ForceNoRequests,
}


impl RecorderMode {
    pub fn should_lookup(self) -> bool {
        match self {
            RecorderMode::RecordOrRequest => true,
            RecorderMode::IgnoreRecordings => false,
            RecorderMode::ForceNoRequests => true,
        }
    }

    pub fn should_request(self) -> bool {
        match self {
            RecorderMode::RecordOrRequest => true,
            RecorderMode::IgnoreRecordings => true,
            RecorderMode::ForceNoRequests => false,
        }
    }
}


#[async_trait]
impl Middleware for RecorderMiddleware {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let request = request.into_memory().await?;
        if self.should_lookup() {
            let recorded = self.request_recorder.get_response(&request);
            if let Some(recorded) = recorded {
                info!(url = request.url().to_string(), "Using recorded response");
                return Ok(recorded.into());
            }
        }
        if !self.should_request() {
            return Err(Error::Protocol(ProtocolError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "No recording found"))));
        }
        let response = next.run(request.clone().into()).await?;
        let response = response.into_content().await?;
        self.request_recorder.record_response(request, response.clone())?;
        Ok(response.into())
    }
}

#[derive(Default)]
pub struct RetryMiddleware {}

#[async_trait]
impl Middleware for RetryMiddleware {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let mut i = 0usize;
        let request = request.into_memory().await?;
        loop {
            match next.run(request.clone().into()).await {
                Ok(response) => return Ok(response),
                Err(err) => {
                    if i == 3 {
                        return Err(err);
                    }
                    i += 1;
                }
            }
        }
    }
}

#[derive(Default)]
pub struct LoggerMiddleware {}

impl LoggerMiddleware {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Middleware for LoggerMiddleware {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let url = request.uri().to_string();
        println!("Request: {:?}", request);
        let res = next.run(request).await;
        println!("Response to {}: {:?}", url, res);
        res
    }
}

#[derive(Default)]
pub struct FollowRedirectsMiddleware {}

impl FollowRedirectsMiddleware {
    pub fn new() -> Self {
        Self {}
    }
}

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
impl Middleware for FollowRedirectsMiddleware {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let request = request.into_memory().await?;
        let mut res = next.run(request.clone().into()).await?;
        let mut allowed_redirects = 10;
        while res.status().is_redirection() {
            if allowed_redirects == 0 {
                return Err(Error::Protocol(ProtocolError::TooManyRedirects));
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