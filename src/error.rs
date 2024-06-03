use crate::{Body, InMemoryResponse, InMemoryResponseExt, Response};
use http::StatusCode;
use std::fmt::{Debug, Display, Formatter};
use std::string::FromUtf8Error;

pub type Result<T = Response, E = Error> = std::result::Result<T, E>;
pub type InMemoryError = Error<InMemoryResponse>;
pub type InMemoryResult<T> = Result<T, InMemoryError>;
pub type ProtocolResult<T> = Result<T, ProtocolError>;

#[derive(Debug)]
pub enum ProtocolError {
    ConnectionError(hyper::Error),
    Utf8Error(FromUtf8Error),
    JsonError(serde_json::Error),
    IoError(std::io::Error),
    TooManyRedirects,
    TooManyRetries,
}

impl std::error::Error for ProtocolError {}

impl Display for ProtocolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::ConnectionError(e) => write!(f, "ConnectionError: {e}"),
            ProtocolError::Utf8Error(e) => write!(f, "Utf8Error: {e}"),
            ProtocolError::JsonError(e) => write!(f, "JsonError: {e}"),
            ProtocolError::IoError(e) => write!(f, "IoError: {e}"),
            ProtocolError::TooManyRedirects => write!(f, "TooManyRedirects"),
            ProtocolError::TooManyRetries => write!(f, "TooManyRetries"),
        }
    }
}

#[derive(Debug)]
pub enum Error<T = Response> {
    Protocol(ProtocolError),
    HttpError(T),
}

impl Error<InMemoryResponse> {
    #[must_use]
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Error::HttpError(r) => Some(r.status()),
            Error::Protocol(_) => None,
        }
    }
}

impl Error {
    /// Get the error status code.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Error::HttpError(r) => Some(r.status()),
            Error::Protocol(_) => None,
        }
    }

    pub async fn into_content(self) -> InMemoryError {
        match self {
            Error::HttpError(r) => {
                let (parts, body) = r.into_parts();
                let content_type = parts.headers.get(http::header::CONTENT_TYPE);
                let body = match body.into_content_type(content_type).await {
                    Ok(body) => body,
                    Err(e) => return e.into(),
                };
                Error::HttpError(InMemoryResponse::from_parts(parts, body))
            }
            Error::Protocol(e) => Error::Protocol(e),
        }
    }
}

impl InMemoryError {
    #[must_use]
    pub fn transform_error<T>(self) -> Error<T>
    where
        T: TryFrom<InMemoryResponse>,
        T::Error: Into<Error<T>>,
    {
        match self {
            InMemoryError::Protocol(e) => Error::Protocol(e),
            InMemoryError::HttpError(e) => match e.try_into() {
                Ok(r) => Error::HttpError(r),
                Err(e) => e.into(),
            },
        }
    }

    #[must_use]
    pub fn into_text(self) -> String {
        match self {
            InMemoryError::Protocol(e) => e.to_string(),
            InMemoryError::HttpError(r) => r.text().unwrap_or_else(|e| format!("Error reading body as text: {e}")),
        }
    }
}

impl From<InMemoryError> for Error {
    fn from(value: InMemoryError) -> Self {
        match value {
            Error::HttpError(r) => {
                let (parts, body) = r.into_parts();
                let body: Body = body.into();
                let r = crate::Response::from_parts(parts, body);
                Error::HttpError(r)
            }
            Error::Protocol(e) => Error::Protocol(e),
        }
    }
}

impl<T: Debug> Display for Error<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::HttpError(r) => write!(f, "HttpError {{ res: {r:?} }}"),
            Error::Protocol(p) => write!(f, "ProtocolError: {p}"),
        }
    }
}

impl<T: Debug> std::error::Error for Error<T> {}

impl serde::de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Protocol(ProtocolError::JsonError(serde_json::Error::custom(msg.to_string())))
    }
}

impl<T> From<serde_json::Error> for Error<T> {
    fn from(value: serde_json::Error) -> Self {
        Error::Protocol(ProtocolError::JsonError(value))
    }
}

impl From<std::io::Error> for ProtocolError {
    fn from(value: std::io::Error) -> Self {
        ProtocolError::IoError(value)
    }
}

impl<T> From<hyper::Error> for Error<T> {
    fn from(value: hyper::Error) -> Self {
        Error::Protocol(ProtocolError::ConnectionError(value))
    }
}

impl<T> From<FromUtf8Error> for Error<T> {
    fn from(value: FromUtf8Error) -> Self {
        Error::Protocol(ProtocolError::Utf8Error(value))
    }
}

impl<T> From<ProtocolError> for Error<T> {
    fn from(value: ProtocolError) -> Self {
        Error::Protocol(value)
    }
}

impl From<hyper::Error> for ProtocolError {
    fn from(value: hyper::Error) -> Self {
        Self::ConnectionError(value)
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(value: serde_json::Error) -> Self {
        Self::JsonError(value)
    }
}

impl From<FromUtf8Error> for ProtocolError {
    fn from(value: FromUtf8Error) -> Self {
        Self::Utf8Error(value)
    }
}
