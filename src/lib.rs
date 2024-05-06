use std::sync::OnceLock;
pub use body::{Body, InMemoryBody};
pub use client::{Client};
pub use error::{Error, InMemoryError, InMemoryResult, Result, ProtocolError, ProtocolResult};
pub use middleware::{Middleware, Retry, Follow, Logger, Recorder, Next};
pub use request::{InMemoryRequest, Request, RequestBuilder};
pub use response::{InMemoryResponse, ResponseExt, InMemoryResponseExt};
pub use http::{header, header::HeaderName, Uri, Method, StatusCode};

pub type Response = http::Response<Body>;

mod client;
mod error;
pub mod recorder;
mod request;
mod response;
pub mod middleware;
mod body;
mod sanitize;
pub mod multipart;

static SHARED_CLIENT: OnceLock<Client> = OnceLock::new();

/// Use this to customize the shared client.
/// Must be called before any requests are made, otherwise it will have no effect.
pub fn init_shared_client(client: Client) {
    let _ = SHARED_CLIENT.set(client);
}

/// Use the shared, global client
pub fn client() -> &'static Client {
    SHARED_CLIENT.get_or_init(|| Client::new())
}