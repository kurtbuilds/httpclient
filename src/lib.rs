pub use ::http::{Method, StatusCode, Uri};
pub use body::{Body, InMemoryBody};
pub use client::Client;
pub use error::{Error, InMemoryError, InMemoryResult, Result};
pub use middleware::Middleware;
pub use request::{InMemoryRequest, Request, RequestBuilder};
pub use response::{InMemoryResponse, Response};

mod body;
mod client;
mod error;
pub mod middleware;
pub mod recorder;
mod request;
mod response;
mod sanitize;
