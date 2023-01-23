use std::hash::Hasher;
use http::HeaderValue;
use hyper::body::{Bytes, HttpBody};
use serde::{Deserialize, Serialize};
use serde::de::{DeserializeOwned, Error};
use serde_json::Value;

use crate::{InMemoryResult, Result};
use crate::error::ProtocolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InMemoryBody {
    Empty,
    Bytes(Vec<u8>),
    Text(String),
    Json(Value),
}

impl Default for InMemoryBody {
    fn default() -> Self {
        InMemoryBody::Empty
    }
}

impl TryInto<String> for InMemoryBody {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<String, Self::Error> {
        match self {
            InMemoryBody::Empty => Ok("".to_string()),
            InMemoryBody::Bytes(b) => {
                let (s, _, _) = encoding_rs::UTF_8.decode(&b);
                Ok(s.to_string())
            }
            InMemoryBody::Text(s) => Ok(s),
            InMemoryBody::Json(val) => Ok(serde_json::to_string(&val)?),
        }
    }
}

impl TryInto<Bytes> for InMemoryBody {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<Bytes, Self::Error> {
        match self {
            InMemoryBody::Empty => Ok(Bytes::new()),
            InMemoryBody::Bytes(b) => Ok(Bytes::from(b)),
            InMemoryBody::Text(s) => Ok(Bytes::from(s)),
            InMemoryBody::Json(val) => Ok(Bytes::from(serde_json::to_string(&val)?)),
        }
    }
}


impl InMemoryBody {
    pub fn empty() -> Self {
        InMemoryBody::Empty
    }

    pub fn is_empty(&self) -> bool {
        use InMemoryBody::*;
        match self {
            Empty => true,
            Bytes(b) => b.is_empty(),
            Text(s) => s.is_empty(),
            Json(_) => false,
        }
    }

    pub fn text(self) -> Result<String> {
        self.try_into()
    }

    pub fn json<T: DeserializeOwned>(self) -> InMemoryResult<T> {
        match self {
            InMemoryBody::Empty => Err(crate::Error::JsonEncodingError(serde_json::Error::custom("Empty body"))),
            InMemoryBody::Bytes(b) => {
                let (s, _, _) = encoding_rs::UTF_8.decode(&b);
                serde_json::from_str(&s).map_err(crate::Error::JsonEncodingError)
            }
            InMemoryBody::Text(t) => {
                serde_json::from_str(&t).map_err(crate::Error::JsonEncodingError)
            }
            InMemoryBody::Json(v) => {
                serde_json::from_value(v).map_err(crate::Error::JsonEncodingError)
            }
        }
    }

    pub fn bytes(self) -> Result<Bytes> {
        self.try_into()
    }

}

impl From<InMemoryBody> for Body {
    fn from(value: InMemoryBody) -> Self {
        Body::InMemory(value)
    }
}

impl std::hash::Hash for InMemoryBody {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use InMemoryBody::*;
        match self {
            Empty => state.write_u8(0),
            Bytes(b) => {
                state.write_u8(1);
                state.write(b.as_slice());
            }
            Text(s) => {
                state.write_u8(2);
                state.write(s.as_bytes());
            }
            Json(v) => {
                state.write_u8(3);
                state.write(v.to_string().as_bytes());
            }
        }
    }
}

#[derive(Debug)]
pub enum Body {
    InMemory(InMemoryBody),
    Hyper(hyper::Body),
}

impl Default for Body {
    fn default() -> Self {
        Body::InMemory(InMemoryBody::default())
    }
}

impl Body {
    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Body::InMemory(InMemoryBody::Bytes(bytes.into()))
    }
    pub fn text(text: impl Into<String>) -> Self {
        Body::InMemory(InMemoryBody::Text(text.into()))
    }
    pub fn json(value: impl Serialize) -> Self {
        Body::InMemory(InMemoryBody::Json(serde_json::to_value(value).unwrap()))
    }

    pub fn empty() -> Self {
        Body::InMemory(InMemoryBody::Empty)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Body::Hyper(b) => b.size_hint().upper() == Some(0),
            Body::InMemory(m) => m.is_empty(),
        }
    }

    pub async fn into_memory(self) -> Result<InMemoryBody, ProtocolError> {
        match self {
            Body::InMemory(m) => Ok(m),
            Body::Hyper(hyper_body) => {
                let bytes = hyper::body::to_bytes(hyper_body).await?;
                Ok(InMemoryBody::Bytes(bytes.to_vec()))
            }
        }
    }

    pub async fn into_content_type(self, content_type: Option<&HeaderValue>) -> Result<InMemoryBody, ProtocolError> {
        match self {
            Body::InMemory(m) => Ok(m),
            Body::Hyper(hyper_body) => {
                let bytes = hyper::body::to_bytes(hyper_body).await?;
                let content_type = content_type.map(|ct| ct.to_str().unwrap().split(';').next().unwrap());
                match content_type {
                    Some("application/json") => {
                        let value = serde_json::from_slice(&bytes)?;
                        Ok(InMemoryBody::Json(value))
                    }
                    Some("application/octet-stream") => Ok(InMemoryBody::Bytes(bytes.to_vec())),
                    _ if bytes.is_empty() => Ok(InMemoryBody::Empty),
                    _ => {
                        let text = String::from_utf8(bytes.to_vec())?;
                        Ok(InMemoryBody::Text(text))
                    },
                }
            }
        }
    }
}

impl From<Body> for hyper::Body {
    fn from(val: Body) -> Self {
        match val {
            Body::Hyper(body) => body,
            Body::InMemory(InMemoryBody::Empty) => hyper::Body::empty(),
            Body::InMemory(InMemoryBody::Text(s)) => hyper::Body::from(s),
            Body::InMemory(InMemoryBody::Bytes(b)) => hyper::Body::from(b),
            Body::InMemory(InMemoryBody::Json(value)) => {
                let b = serde_json::to_vec(&value).unwrap();
                hyper::Body::from(b)
            },
        }
    }
}