use http::HeaderValue;
use hyper::body::HttpBody;

pub use memory::*;

use crate::Result;
use crate::error::ProtocolError;

mod memory;

#[derive(Debug)]
pub enum Body {
    InMemory(InMemoryBody),
    Hyper(hyper::Body),
}


impl Body {
    pub fn new_empty() -> Self {
        Body::InMemory(InMemoryBody::new_empty())
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

impl From<Body> for hyper::Body {
    fn from(val: Body) -> Self {
        match val {
            Body::Hyper(body) => body,
            Body::InMemory(body) => body.into(),
        }
    }
}

impl From<InMemoryBody> for hyper::Body {
    fn from(val: InMemoryBody) -> Self {
        match val {
            InMemoryBody::Empty => hyper::Body::empty(),
            InMemoryBody::Text(s) => hyper::Body::from(s),
            InMemoryBody::Bytes(b) => hyper::Body::from(b),
            InMemoryBody::Json(value) => {
                let b = serde_json::to_vec(&value).unwrap();
                hyper::Body::from(b)
            },
        }
    }
}

impl From<hyper::Body> for Body {
    fn from(val: hyper::Body) -> Self {
        Body::Hyper(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization() {
        let body = InMemoryBody::new_json(serde_json::json!({
            "foo": "bar"
        }));
        assert_eq!(serde_json::to_string(&body).unwrap(), r#"{"foo":"bar"}"#);
    }
}