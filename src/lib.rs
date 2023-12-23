use std::sync::OnceLock;
pub use body::{Body, InMemoryBody};
pub use client::{Client};
pub use error::{Error, InMemoryError, InMemoryResult, Result};
pub use middleware::{Middleware, Retry, Follow, Logger, Next};
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

static GLOBAL_CLIENT: OnceLock<Client> = OnceLock::new();

/// Use the shared, global client
pub fn client() -> &'static Client {
    GLOBAL_CLIENT.get_or_init(|| Client::new())
}