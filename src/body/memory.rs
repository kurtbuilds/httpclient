use hyper::body::Bytes;
use std::hash::Hasher;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde::de::{DeserializeOwned, Error};
use crate::InMemoryResult;
use crate::sanitize::sanitize_value;

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

    fn try_into(self) -> Result<String, Self::Error> {
        match self {
            InMemoryBody::Empty => Ok("".to_string()),
            InMemoryBody::Bytes(b) => {
                String::from_utf8(b).map_err(crate::Error::Utf8Error)
            }
            InMemoryBody::Text(s) => Ok(s),
            InMemoryBody::Json(val) => serde_json::to_string(&val).map_err(crate::Error::JsonEncodingError)
        }
    }
}

impl TryInto<Bytes> for InMemoryBody {
    type Error = crate::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        match self {
            InMemoryBody::Empty => Ok(Bytes::new()),
            InMemoryBody::Bytes(b) => Ok(Bytes::from(b)),
            InMemoryBody::Text(s) => Ok(Bytes::from(s)),
            InMemoryBody::Json(val) => Ok(Bytes::from(serde_json::to_string(&val)?)),
        }
    }
}


impl InMemoryBody {
    pub fn new_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        InMemoryBody::Bytes(bytes.into())
    }

    pub fn new_text(text: impl Into<String>) -> Self {
        InMemoryBody::Text(text.into())
    }

    pub fn new_json(value: impl Serialize) -> Self {
        InMemoryBody::Json(serde_json::to_value(value).unwrap())
    }

    pub fn new_empty() -> Self {
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

    pub fn text(self) -> crate::Result<String> {
        self.try_into()
    }

    pub fn json<T: DeserializeOwned>(self) -> InMemoryResult<T> {
        match self {
            InMemoryBody::Empty => Err(crate::Error::JsonEncodingError(serde_json::Error::custom("Empty body"))),
            InMemoryBody::Bytes(b) => {
                serde_json::from_slice(&b).map_err(crate::Error::JsonEncodingError)
            }
            InMemoryBody::Text(t) => {
                serde_json::from_str(&t).map_err(crate::Error::JsonEncodingError)
            }
            InMemoryBody::Json(v) => {
                serde_json::from_value(v).map_err(crate::Error::JsonEncodingError)
            }
        }
    }

    pub fn bytes(self) -> crate::Result<Bytes> {
        self.try_into()
    }

    pub fn sanitize(&mut self) {
        if let InMemoryBody::Json(value) = self {
            sanitize_value(value)
        }
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
