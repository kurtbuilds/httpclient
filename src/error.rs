use std::fmt::{Display, Formatter};
use crate::Response;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Generic(String),
    #[error("Hyper Error: {0}")]
    HyperError(#[from] hyper::Error),
    #[error("Http Error: {0}")]
    HttpError(#[from] hyper::http::Error),
    #[error("Utf8 Error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("JsonError: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("io::Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("HttpError")]
    HttpStatusError(Response),
}


impl serde::de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Generic(msg.to_string())
    }
}