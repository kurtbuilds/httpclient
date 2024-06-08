use std::sync::OnceLock;

use async_trait::async_trait;
use http::header::CONTENT_TYPE;
use tracing::info;

use crate::{Body, InMemoryRequest, InMemoryResponse, Middleware, Response};
use crate::error::ProtocolResult;
use crate::middleware::Next;
use crate::middleware::ProtocolError;
use crate::recorder::{HashableRequest, RequestRecorder};

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
    #[must_use]
    pub fn should_lookup(self) -> bool {
        match self {
            RecorderMode::IgnoreRecordings => false,
            RecorderMode::ForceNoRequests | RecorderMode::RecordOrRequest => true,
        }
    }

    #[must_use]
    pub fn should_request(self) -> bool {
        match self {
            RecorderMode::IgnoreRecordings | RecorderMode::RecordOrRequest => true,
            RecorderMode::ForceNoRequests => false,
        }
    }
}

static SHARED_RECORDER: OnceLock<RequestRecorder> = OnceLock::new();

pub fn shared_recorder() -> &'static RequestRecorder {
    SHARED_RECORDER.get_or_init(RequestRecorder::new)
}

#[derive(Default, Copy, Clone, Debug)]
/// This middleware caches requests to the local filesystem. Subsequent requests will return results
/// from the filesystem, and not touch the remote server.
///
/// The recordings are sanitized to hide secrets.
///
/// Use `.mode()` to configure the behavior:
/// - `RecorderMode::RecordOrRequest` (default): Will check for recordings, but will make the request if no recording is found.
/// - `RecorderMode::IgnoreRecordings`: Always make the request. (Use to force refresh recordings.)
/// - `RecorderMode::ForceNoRequests`: Fail if no recording is found. (Use to run tests without hitting the remote server.)
pub struct Recorder {
    pub mode: RecorderMode,
}

impl Recorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn mode(mut self, mode: RecorderMode) -> Self {
        self.mode = mode;
        self
    }

    fn should_lookup(self) -> bool {
        self.mode.should_lookup()
    }

    fn should_request(self) -> bool {
        self.mode.should_request()
    }
}

#[async_trait]
impl Middleware for Recorder {
    #[allow(clippy::similar_names)]
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let recorder = shared_recorder();

        let request = HashableRequest(request);
        if self.should_lookup() {
            let recorded = recorder.get_response(&request);

            if let Some(recorded) = recorded {
                info!(url = request.uri().to_string(), "Using recorded response");

                let (parts, body) = recorded.into_parts();
                return Ok(Response::from_parts(parts, Body::InMemory(body)));
            }
        }

        if !self.should_request() {
            return Err(ProtocolError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "No recording found")));
        }

        let response = next.run(request.clone()).await?;
        let (parts, body) = response.into_parts();
        let content_type = parts.headers.get(CONTENT_TYPE);
        let body = body.into_content_type(content_type).await?;
        let response = InMemoryResponse::from_parts(parts, body);

        recorder.record_response(request.0, response.clone())?;

        let (parts, body) = response.into_parts();
        Ok(Response::from_parts(parts, Body::InMemory(body)))
    }
}