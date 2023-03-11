use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};
use std::string::FromUtf8Error;

use http::StatusCode;

use crate::{Body, InMemoryBody, InMemoryResponse};

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub type InMemoryError = Error<InMemoryBody>;
pub type InMemoryResult<T> = Result<T, InMemoryError>;

#[derive(Debug)]
pub enum ProtocolError {
    HttpProtocol(hyper::Error),
    Utf8(FromUtf8Error),
    JsonEncoding(serde_json::Error),
}

impl StdError for ProtocolError {}

impl Display for ProtocolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::HttpProtocol(e) => write!(f, "HttpProtocolError: {}", e),
            ProtocolError::Utf8(e) => write!(f, "Utf8Error: {}", e),
            ProtocolError::JsonEncoding(e) => write!(f, "JsonEncodingError: {}", e),
        }
    }
}

pub enum Error<T = Body> {
    Custom(String),
    TooManyRedirects,
    HttpProtocol(hyper::Error),
    Utf8Error(FromUtf8Error),
    JsonEncoding(serde_json::Error),
    IoError(std::io::Error),
    HttpError(crate::Response<T>),
}

impl<T> Error<T> {
    pub fn custom(msg: &str) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl Error {
    /// Get the error status code.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Error::HttpError(r) => Some(r.status()),
            _ => None,
        }
    }

    pub async fn into_memory(self) -> Error<InMemoryBody> {
        match self {
            Error::HttpError(r) => {
                let (parts, body) = r.into_parts();
                let body = match body.into_memory().await {
                    Ok(body) => body,
                    Err(e) => return e.into(),
                };
                Error::HttpError(InMemoryResponse::from_parts(parts, body))
            }
            Error::Custom(e) => Error::Custom(e),
            Error::TooManyRedirects => Error::TooManyRedirects,
            Error::HttpProtocol(h) => Error::HttpProtocol(h),
            Error::Utf8Error(u) => Error::Utf8Error(u),
            Error::JsonEncoding(e) => Error::JsonEncoding(e),
            Error::IoError(i) => Error::IoError(i),
        }
    }
}

impl From<InMemoryError> for Error {
    fn from(value: InMemoryError) -> Self {
        match value {
            Error::HttpError(r) => Error::HttpError(r.into()),
            Error::Custom(e) => Error::Custom(e),
            Error::TooManyRedirects => Error::TooManyRedirects,
            Error::HttpProtocol(h) => Error::HttpProtocol(h),
            Error::Utf8Error(u) => Error::Utf8Error(u),
            Error::JsonEncoding(e) => Error::JsonEncoding(e),
            Error::IoError(i) => Error::IoError(i),
        }
    }
}

impl<T: Debug> Debug for Error<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Custom(msg) => write!(f, "Custom: {}", msg),
            Error::HttpProtocol(e) => write!(f, "HttpProtocolError: {}", e),
            Error::Utf8Error(e) => write!(f, "Utf8Error: {}", e),
            Error::JsonEncoding(e) => write!(f, "JsonEncodingError: {}", e),
            Error::IoError(e) => write!(f, "IoError: {}", e),
            Error::HttpError(r) => {
                write!(
                    f,
                    "HttpError {{ status: {}, headers: {:?}, body: {:?} }}",
                    r.parts.status, r.parts.headers, r.body
                )
            }
            Error::TooManyRedirects => write!(f, "TooManyRedirectsError"),
        }
    }
}

impl<T: Debug> Display for Error<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Custom(msg) => write!(f, "{}", msg),
            Error::HttpProtocol(e) => write!(f, "HttpProtocolError: {}", e),
            Error::Utf8Error(e) => write!(f, "Utf8Error: {}", e),
            Error::JsonEncoding(e) => write!(f, "JsonEncodingError: {}", e),
            Error::IoError(e) => write!(f, "IoError: {}", e),
            Error::HttpError(r) => {
                write!(
                    f,
                    "HttpError {{ status: {}, headers: {:?}, body: {:?} }}",
                    r.parts.status, r.parts.headers, r.body
                )
            }
            Error::TooManyRedirects => write!(f, "Too many redirects"),
        }
    }
}

impl<T: Debug> StdError for Error<T> {}

impl serde::de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl<T> From<serde_json::Error> for Error<T> {
    fn from(value: serde_json::Error) -> Self {
        Error::JsonEncoding(value)
    }
}

impl<T> From<std::io::Error> for Error<T> {
    fn from(value: std::io::Error) -> Self {
        Error::IoError(value)
    }
}

impl<T> From<hyper::Error> for Error<T> {
    fn from(value: hyper::Error) -> Self {
        Error::HttpProtocol(value)
    }
}

impl<T> From<FromUtf8Error> for Error<T> {
    fn from(value: FromUtf8Error) -> Self {
        Error::Utf8Error(value)
    }
}

impl<T> From<ProtocolError> for Error<T> {
    fn from(value: ProtocolError) -> Self {
        match value {
            ProtocolError::HttpProtocol(e) => Error::HttpProtocol(e),
            ProtocolError::Utf8(e) => Error::Utf8Error(e),
            ProtocolError::JsonEncoding(e) => Error::JsonEncoding(e),
        }
    }
}

impl From<hyper::Error> for ProtocolError {
    fn from(value: hyper::Error) -> Self {
        Self::HttpProtocol(value)
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(value: serde_json::Error) -> Self {
        Self::JsonEncoding(value)
    }
}

impl From<FromUtf8Error> for ProtocolError {
    fn from(value: FromUtf8Error) -> Self {
        Self::Utf8(value)
    }
}
