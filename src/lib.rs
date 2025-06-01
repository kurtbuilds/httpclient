#![deny(clippy::all, clippy::pedantic, clippy::unwrap_used)]
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc, clippy::missing_panics_doc)]

pub use body::{Body, InMemoryBody};
#[cfg(feature = "stream")]
pub use body::DataStream;
pub use client::Client;
pub use error::{Error, InMemoryError, InMemoryResult, ProtocolError, ProtocolResult, Result};
pub use http::{header, header::HeaderName, Method, StatusCode, Uri};
pub use middleware::{Follow, Logger, Middleware, Next, Recorder, Retry, TotalTimeout};
pub use request::{InMemoryRequest, Request, RequestBuilder, RequestBuilderExt, RequestExt};
pub use response::{InMemoryResponse, InMemoryResponseExt, ResponseExt};
use std::sync::OnceLock;

pub mod header_ext {
    use http::HeaderName;

    pub const SUBJECT: HeaderName = HeaderName::from_static("subject");
    pub const FROM: HeaderName = HeaderName::from_static("from");
    pub const TO: HeaderName = HeaderName::from_static("to");
    pub const CONTENT_TRANSFER_ENCODING: HeaderName = HeaderName::from_static("content-transfer-encoding");
}
pub type Response<T = Body> = http::Response<T>;

mod body;
mod client;
mod error;
pub mod middleware;
pub mod multipart;
pub mod recorder;
mod request;
mod response;
mod sanitize;
#[cfg(feature = "oauth2")]
pub mod oauth2;

static SHARED_CLIENT: OnceLock<Client> = OnceLock::new();

/// Use this to customize the shared client.
/// Must be called before any requests are made, otherwise it will have no effect.
pub fn init_shared_client(client: Client) {
    let _ = SHARED_CLIENT.set(client);
}

/// Use the shared, global client
pub fn client() -> &'static Client {
    SHARED_CLIENT.get_or_init(Client::new)
}
