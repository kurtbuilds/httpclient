use std::{convert::Infallible, net::SocketAddr};

pub use http::{Method, Uri};

pub use body::Body;
pub use middleware::Middleware;
pub use request::{Request, RequestBuilder};
pub use response::Response;

pub use crate::client::Client;
pub use crate::error::Error;

pub mod proxy_server;
mod client;
mod error;
pub mod request_recorder;
mod request;
mod response;
pub mod middleware;
mod body;
mod headers;

