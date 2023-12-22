use std::str::FromStr;
use crate::{Body, Error, Response, ResponseExt};

use async_trait::async_trait;
use http::Uri;
use crate::client::Client;
use crate::error::ProtocolError;
use crate::request::{Request};
use crate::recorder::RequestRecorder;
use tracing::info;
use crate::response::{clone_inmemory_response, response_into_content};

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
            self.client.start_request(request).await
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
                let (parts, body) = recorded.into_parts();
                let body: Body = body.into();
                let recorded = Response::from_parts(parts, body);
                return Ok(recorded);
            }
        }
        if !self.should_request() {
            return Err(Error::Protocol(ProtocolError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "No recording found"))));
        }
        let response = next.run(request.clone().into()).await?;
        let response = response_into_content(response).await?;
        self.request_recorder.record_response(request, clone_inmemory_response(&response))?;
        let (parts, body) = response.into_parts();
        let body: Body = body.into();
        let response = Response::from_parts(parts, body);
        Ok(response)
    }
}

pub struct Retry;

#[async_trait]
impl Middleware for Retry {
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

pub struct Logger;

#[async_trait]
impl Middleware for Logger {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let url = request.uri().to_string();
        let method = request.method().as_str().to_uppercase();
        let version = request.version();
        let headers = request.headers()
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap()))
            .collect::<Vec<_>>()
            .join("\n");
        let body = request.body();
        println!("Request:
{method} {url} HTTP/{version:?}
{headers}");
        if !body.is_empty() {
            println!("{:?}", body);
        }
        let res = next.run(request).await;
        // let version = res.v
        match res {
            Err(Error::Protocol(e)) => {
                println!("Response to {url}:\n{e}");
                Err(Error::Protocol(e))
            },
            | Ok(res)
            | Err(Error::HttpError(res)) => {
                let version = res.version();
                let status = res.status();
                let headers = res.headers()
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap()))
                    .collect::<Vec<_>>()
                    .join("\n");
                println!("Response to {url}:
HTTP/{version:?} {status}
{headers}");
                println!("{:?}", res.body());
                res.error_for_status()
            }
        }
    }
}

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

pub struct Oauth2 {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[async_trait]
impl Middleware for Oauth2 {

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