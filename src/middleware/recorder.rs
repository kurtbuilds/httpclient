use std::sync::OnceLock;

use async_trait::async_trait;
use tracing::info;

use crate::{InMemoryRequest, Middleware, Response};
use crate::error::ProtocolResult;
use crate::middleware::ProtocolError;
use crate::middleware::Next;
use crate::recorder::RequestRecorder;
use crate::response::{clone_inmemory_response, mem_response_into_hyper, response_into_content};

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


static SHARED_RECORDER: OnceLock<RequestRecorder> = OnceLock::new();

pub fn shared_recorder() -> &'static RequestRecorder {
    SHARED_RECORDER.get_or_init(|| RequestRecorder::new())
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
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let recorder = shared_recorder();
        if self.should_lookup() {
            let recorded = recorder.get_response(&request);
            if let Some(recorded) = recorded {
                info!(url = request.url().to_string(), "Using recorded response");
                return Ok(mem_response_into_hyper(recorded));
            }
        }
        if !self.should_request() {
            return Err(ProtocolError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "No recording found")));
        }
        let response = next.run(request.clone().into()).await?;
        let response = response_into_content(response).await?;
        recorder.record_response(request, clone_inmemory_response(&response))?;
        Ok(mem_response_into_hyper(response))
    }
}