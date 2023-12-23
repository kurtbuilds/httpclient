use std::sync::OnceLock;

use async_trait::async_trait;
use tracing::info;

use crate::{Body, Error, Middleware, Request, Response};
use crate::error::ProtocolError;
use crate::middleware::Next;
use crate::recorder::RequestRecorder;
use crate::response::{clone_inmemory_response, response_into_content};

#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
pub enum RecorderMode {
    /// Default. Will check for recordings, but will make the request if no recording is found.
    #[default]
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

/// This middleware caches requests to the local filesystem. Subsequent requests will return results
/// from the filesystem, and not touch the remote server.
///
/// The recordings are sanitized to hide secrets.
///
/// Use `.mode()` to configure the behavior:
/// - `RecorderMode::RecordOrRequest` (default): Will check for recordings, but will make the request if no recording is found.
/// - `RecorderMode::IgnoreRecordings`: Always make the request. (Use to force refresh recordings.)
/// - `RecorderMode::ForceNoRequests`: Fail if no recording is found. (Use to run tests without hitting the remote server.)
#[derive(Debug, Clone)]
pub struct RecorderMiddleware {
    mode: RecorderMode,
    pub request_recorder: RequestRecorder,
}

impl Default for RecorderMiddleware {
    fn default() -> Self {
        Self {
            mode: RecorderMode::RecordOrRequest,
            request_recorder: RequestRecorder::new(),
        }
    }
}

impl RecorderMiddleware {
    pub fn new() -> Self {
        Self::default()
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

static SHARED_RECORDER: OnceLock<RequestRecorder> = OnceLock::new();

pub fn shared_recorder() -> &'static RequestRecorder {
    SHARED_RECORDER.get_or_init(|| RequestRecorder::new())
}

#[derive(Default, Copy, Clone, Debug)]
/// This middleware caches requests to the local filesystem. Subsequent requests will return results
pub struct Recorder {
    pub mode: RecorderMode,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            mode: Default::default(),
        }
    }

    pub fn mode(mut self, mode: RecorderMode) -> Self {
        self.mode = mode;
        self
    }

    fn should_lookup(&self) -> bool {
        self.mode.should_lookup()
    }

    fn should_request(&self) -> bool {
        self.mode.should_request()
    }
}

#[async_trait]
impl Middleware for Recorder {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let request = request.into_memory().await?;
        if self.should_lookup() {
            let recorded = shared_recorder().get_response(&request);
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
        shared_recorder().record_response(request, clone_inmemory_response(&response))?;
        let (parts, body) = response.into_parts();
        let body: Body = body.into();
        let response = Response::from_parts(parts, body);
        Ok(response)
    }
}