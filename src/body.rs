use http::HeaderValue;
use http_body_util::BodyExt;
use http_body::Body as HttpBodyTrait;

pub use memory::*;

use crate::error::ProtocolResult;

mod memory;

#[derive(Debug)]
pub enum Body {
    InMemory(InMemoryBody),
    Incoming(hyper::body::Incoming),
}

impl Body {
    pub fn is_empty(&self) -> bool {
        match self {
            Body::Incoming(b) => b.size_hint().upper() == Some(0),
            Body::InMemory(m) => m.is_empty(),
        }
    }

    pub async fn into_memory(self) -> ProtocolResult<InMemoryBody> {
        match self {
            Body::InMemory(m) => Ok(m),
            Body::Incoming(incoming_body) => {
                let bytes = incoming_body.collect().await?.to_bytes();
                Ok(InMemoryBody::Bytes(bytes.to_vec()))
            }
        }
    }

    pub async fn into_content_type(self, content_type: Option<&HeaderValue>) -> ProtocolResult<InMemoryBody> {
        match self {
            Body::InMemory(m) => Ok(m),
            Body::Incoming(incoming_body) => {
                let bytes = incoming_body.collect().await?.to_bytes();
                Self::process_bytes(bytes, content_type)
            }
        }
    }

    fn process_bytes(bytes: bytes::Bytes, content_type: Option<&HeaderValue>) -> ProtocolResult<InMemoryBody> {
        let content_type = content_type.and_then(|t| t.to_str().ok()).and_then(|t| t.split(';').next());
        match content_type {
            Some("application/json") => {
                let value = serde_json::from_slice(&bytes)?;
                Ok(InMemoryBody::Json(value))
            }
            Some("application/octet-stream") => Ok(InMemoryBody::Bytes(bytes.to_vec())),
            _ if bytes.is_empty() => Ok(InMemoryBody::Empty),
            _ => match String::from_utf8(bytes.to_vec()) {
                Ok(text) => Ok(InMemoryBody::Text(text)),
                Err(e) => {
                    let bytes = e.into_bytes();
                    Ok(InMemoryBody::Bytes(bytes))
                }
            },
        }
    }
}

impl Default for Body {
    fn default() -> Self {
        Body::InMemory(InMemoryBody::default())
    }
}

impl From<InMemoryBody> for Body {
    fn from(value: InMemoryBody) -> Self {
        Body::InMemory(value)
    }
}

impl From<Body> for http_body_util::Full<bytes::Bytes> {
    fn from(val: Body) -> Self {
        match val {
            Body::InMemory(body) => body.into(),
            Body::Incoming(_) => panic!("Cannot convert Incoming body to Full body directly"),
        }
    }
}

impl From<InMemoryBody> for http_body_util::Full<bytes::Bytes> {
    fn from(val: InMemoryBody) -> Self {
        match val {
            InMemoryBody::Empty => http_body_util::Full::new(bytes::Bytes::new()),
            InMemoryBody::Text(s) => http_body_util::Full::new(bytes::Bytes::from(s)),
            InMemoryBody::Bytes(b) => http_body_util::Full::new(bytes::Bytes::from(b)),
            InMemoryBody::Json(value) => {
                let b = serde_json::to_vec(&value).unwrap();
                http_body_util::Full::new(bytes::Bytes::from(b))
            }
        }
    }
}


impl From<hyper::body::Incoming> for Body {
    fn from(val: hyper::body::Incoming) -> Self {
        Body::Incoming(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_serialization() {
        let body = InMemoryBody::Json(json!({
            "foo": "bar"
        }));
        assert_eq!(serde_json::to_string(&body).expect("Unable to deserialize JSON"), r#"{"foo":"bar"}"#);
    }
}
