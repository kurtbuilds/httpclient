use crate::sanitize::sanitize_value;
use crate::InMemoryResult;
use hyper::body::Bytes;
use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::hash::Hasher;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(Default)]
pub enum InMemoryBody {
    #[default]
    Empty,
    // json must come before bytes, otherwise Recorder deserialization gets messed up, see
    // response::memory::test_deserialize
    Json(Value),
    Bytes(Vec<u8>),
    Text(String),
}

impl TryInto<String> for InMemoryBody {
    type Error = crate::InMemoryError;

    fn try_into(self) -> InMemoryResult<String> {
        match self {
            InMemoryBody::Empty => Ok(String::new()),
            InMemoryBody::Bytes(b) => String::from_utf8(b).map_err(std::convert::Into::into),
            InMemoryBody::Text(s) => Ok(s),
            InMemoryBody::Json(val) => match val {
                Value::String(s) => Ok(s),
                _ => serde_json::to_string(&val).map_err(std::convert::Into::into),
            },
        }
    }
}

impl TryInto<Bytes> for InMemoryBody {
    type Error = crate::InMemoryError;

    fn try_into(self) -> InMemoryResult<Bytes> {
        match self {
            InMemoryBody::Empty => Ok(Bytes::new()),
            InMemoryBody::Bytes(b) => Ok(Bytes::from(b)),
            InMemoryBody::Text(s) => Ok(Bytes::from(s)),
            InMemoryBody::Json(val) => {
                if let Value::Array(a) = &val {
                    if a.iter().all(|v| v.is_number()) {
                        let mut bytes = Vec::with_capacity(a.len());
                        for v in a {
                            bytes.push(v.as_u64().unwrap() as u8);
                        }
                        return Ok(Bytes::from(bytes));
                    }
                }
                Ok(Bytes::from(serde_json::to_string(&val)?))
            }
        }
    }
}

impl InMemoryBody {
    pub fn is_empty(&self) -> bool {
        match self {
            InMemoryBody::Empty => true,
            InMemoryBody::Bytes(b) => b.is_empty(),
            InMemoryBody::Text(s) => s.is_empty(),
            InMemoryBody::Json(_) => false,
        }
    }

    pub fn text(&self) -> InMemoryResult<Cow<str>> {
        match self {
            InMemoryBody::Empty => Ok(Cow::Borrowed("")),
            InMemoryBody::Json(value) => serde_json::to_string(&value).map(Cow::Owned).map_err(Into::into),
            InMemoryBody::Bytes(items) => std::str::from_utf8(items).map(Cow::Borrowed).map_err(Into::into),
            InMemoryBody::Text(s) => Ok(Cow::Borrowed(s.as_str())),
        }
    }

    pub fn into_text(self) -> InMemoryResult<String> {
        self.try_into()
    }

    pub fn json<T: DeserializeOwned>(self) -> serde_json::Result<T> {
        match self {
            InMemoryBody::Empty => Err(serde_json::Error::custom("Empty body")),
            InMemoryBody::Bytes(b) => serde_json::from_slice(&b),
            InMemoryBody::Text(t) => serde_json::from_str(&t),
            InMemoryBody::Json(v) => serde_json::from_value(v),
        }
    }

    pub fn bytes(self) -> InMemoryResult<Bytes> {
        self.try_into()
    }

    pub fn sanitize(&mut self) {
        if let InMemoryBody::Json(value) = self {
            sanitize_value(value);
        }
    }
}

impl std::hash::Hash for InMemoryBody {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            InMemoryBody::Empty => {}
            // InMemoryBody::Empty => state.write_u8(0),
            InMemoryBody::Bytes(b) => {
                // state.write_u8(1);
                state.write(b.as_slice());
            }
            InMemoryBody::Text(s) => {
                // state.write_u8(2);
                state.write(s.as_bytes());
            }
            InMemoryBody::Json(serde_json::Value::String(s)) => {
                state.write(s.as_bytes());
                // state.write_u8(3);
                // state.write(v.to_string().as_bytes());
            }
            InMemoryBody::Json(v) => {
                state.write(v.to_string().as_bytes());
            }
        }
    }
}

impl Into<InMemoryBody> for String {
    fn into(self) -> InMemoryBody {
        InMemoryBody::Text(self)
    }
}

impl Into<InMemoryBody> for Vec<u8> {
    fn into(self) -> InMemoryBody {
        InMemoryBody::Bytes(self)
    }
}
