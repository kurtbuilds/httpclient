use std::str::FromStr;
use crate::Error;

use crate::response::Response;
use async_trait::async_trait;
use http::Uri;
use crate::client::Client;
use crate::request::{Request};
use crate::request_recorder::RequestRecorder;


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
            self.client.send(request).await
        }
    }
}

#[async_trait]
pub trait Middleware: Send + Sync {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        next.run(request).await
    }
}

pub struct RecorderMiddleware {
    mode: RecorderMode,
    request_recorder: RequestRecorder,
}

impl RecorderMiddleware {
    pub fn new() -> Self {
        Self {
            mode: RecorderMode::RecordOrRequest,
            request_recorder: RequestRecorder::new(),
        }
    }

    pub fn with_mode(mode: RecorderMode) -> Self {
        Self {
            mode,
            request_recorder: RequestRecorder::new(),
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
    RecordOrRequest,
    IgnoreRecordings,
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
        let request = request.into_infallible_cloneable().await?;
        if self.should_lookup() {
            let recorded = self.request_recorder.recorded_response(&request);
            if recorded.is_some() {
                return Ok(recorded.unwrap());
            }
        }
        if !self.should_request() {
            return Err(Error::Generic("No recording found".to_string()));
        }
        let mut response = next.run(request.try_clone().unwrap()).await;
        if response.is_ok() {
            response = self.request_recorder.record_response(request, response.unwrap()).await;
        }
        response
    }
}

pub struct RetryMiddleware {}

#[async_trait]
impl Middleware for RetryMiddleware {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let mut i = 0usize;
        let request = request.into_infallible_cloneable().await?;
        loop {
            match next.run(request.try_clone().unwrap()).await {
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

pub struct LoggerMiddleware {}

impl LoggerMiddleware {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Middleware for LoggerMiddleware {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let url = request.url().to_string();
        println!("Request: {:?}", request);
        let res = next.run(request).await;
        println!("Response to {}: {:?}", url, res);
        res
    }
}

pub struct FollowRedirectsMiddleware {}

impl FollowRedirectsMiddleware {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Middleware for FollowRedirectsMiddleware {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let request = request.into_infallible_cloneable().await?;
        let mut res = next.run(request.try_clone().unwrap()).await?;
        let mut allowed_redirects = 10;
        while res.status().is_redirection() {
            if allowed_redirects == 0 {
                return Err(Error::Generic("Too many redirects".to_string()));
            }
            let url = res.headers().get(http::header::LOCATION).unwrap().to_str().unwrap();
            let url = Uri::from_str(url).unwrap();
            let (mut parts, body) = request.try_clone().unwrap().into_parts();
            parts.uri = url;
            let request = Request::from_parts(parts, body);
            allowed_redirects -= 1;
            res = next.run(request).await?;
        }
        Ok(res)
    }
}