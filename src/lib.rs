pub use ::http::{Method, StatusCode, Uri};

pub use body::{Body, InMemoryBody};
pub use client::Client;
pub use error::{Error, InMemoryError, InMemoryResult, Result};
pub use middleware::Middleware;
pub use request::{InMemoryRequest, Request, RequestBuilder};
pub use response::{InMemoryResponse, ResponseExt, InMemoryResponseExt};

pub type Response = http::Response<Body>;

mod client;
mod error;
pub mod recorder;
mod request;
mod response;
pub mod middleware;
mod body;
mod sanitize;