use std::fmt::{Display};
use std::string::FromUtf8Error;
use crate::Response;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Generic(String),
    #[error("Hyper Error: {0}")]
    HyperError(#[from] hyper::Error),
    #[error("Http Error: {0}")]
    HttpError(#[from] http::Error),
    #[error("Http Error: {0}")]
    Utf8Error(#[from] FromUtf8Error),
    #[error("JsonError: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("io::Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("ApplicationJsonError {status}")]
    ApplicationErrorJson {
        status: http::StatusCode,
        headers: hyper::HeaderMap,
        body: serde_json::Value,
    },
    #[error("ApplicationTextError {status}: {body}")]
    ApplicationErrorText {
        status: http::StatusCode,
        headers: hyper::HeaderMap,
        body: String,
    },
}


impl serde::de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Generic(msg.to_string())
    }
}