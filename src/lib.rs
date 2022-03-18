#![allow(unused)]
pub mod proxy_server;
// pub mod connection_recorder;
// mod example_service_client;
mod client;
mod error;
pub mod request_recorder;
mod request;
mod response;
pub mod middleware;
mod body;
mod headers;

pub use crate::error::Error;
pub use middleware::Middleware;
pub use body::Body;

use std::{convert::Infallible, net::SocketAddr};
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
pub use request::{Request, RequestBuilder};
pub use response::Response;
pub use http::Method;

pub use crate::client::Client;


async fn handle(_: hyper::Request<hyper::Body>) -> Result<hyper::Response<hyper::Body>, Infallible> {
    Ok(hyper::Response::new("Hello, World!".into()))
}

pub async fn make_server() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(handle))
    });

    let server = Server::bind(&addr).serve(make_svc);
    server.await.unwrap();
}


// #[cfg(test)]
// mod tests {
//     use anyhow::Result;
//     use super::*;
//
//     #[tokio::test]
//     async fn it_works() -> Result<()> {
//         tokio::spawn(make_server());
//         // let res = reqwest::get("https://www.rust-lang.org/")
//         let res = reqwest::get("http://localhost:3000/hello")
//             .await?
//             .error_for_status()?;
//         let status = res.status().as_u16();
//         let text = res.text().await?;
//         assert_eq!(status, 200);
//         assert_eq!(text, "Hello, Worldf!");
//         Ok(())
//     }
// }
