extern crate core;

pub use ::http::{Method, Uri};

pub use body::{Body, InMemoryBody};
pub use middleware::Middleware;
pub use request::{Request, RequestBuilder, InMemoryRequest};
pub use response::{Response, InMemoryResponse};

pub use crate::client::Client;
pub use crate::error::{Error, Result};

mod client;
mod error;
pub mod request_recorder;
mod request;
mod response;
pub mod middleware;
mod body;
mod http;
mod sanitize;

