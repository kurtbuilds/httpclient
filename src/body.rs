
use std::hash::Hasher;
use std::pin::Pin;

use std::task::{Context, Poll};
use encoding_rs::Encoding;
use http::{HeaderMap, HeaderValue};
use hyper::body::{Bytes, HttpBody, SizeHint};
use serde::{Serialize, Deserialize};
use serde::de::{DeserializeOwned, Error};
use serde_json::Value;
use crate::{Result};
use crate::error::ProtocolError;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InMemoryBody {
    Empty,
    Bytes(Vec<u8>),
    Text(String),
    Json(Value),
}

impl InMemoryBody {
    pub fn is_empty(&self) -> bool {
        use InMemoryBody::*;
        match self {
            Empty => true,
            Bytes(b) => b.is_empty(),
            Text(s) => s.is_empty(),
            Json(_) => false,
        }
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

    pub async fn into_memory(self, content_type: Option<&HeaderValue>) -> std::result::Result<InMemoryBody, ProtocolError> {
        match self {
            Body::InMemory(m) => Ok(m),
            Body::Hyper(hyper_body) => {
                let bytes = hyper::body::to_bytes(hyper_body).await?;
                let encoding = Encoding::for_label(&[]).unwrap_or(encoding_rs::UTF_8);
                let (text, _, _) = encoding.decode(&bytes);
                let content_type = content_type.map(|ct| ct.to_str().unwrap().split(';').next().unwrap());
                match content_type {
                    // consider if we should always return text
                    Some("application/json") => Ok(InMemoryBody::Json(serde_json::from_str::<Value>(text.as_ref())?)),
                    _ => Ok(InMemoryBody::Text(text.to_string())),
                }
            }
        }
    }

    pub async fn into_text(self) -> Result<String> {
        match self {
            Body::Hyper(hyper_body) => {
                let bytes = hyper::body::to_bytes(hyper_body).await?;
                let encoding = Encoding::for_label(&[]).unwrap_or(encoding_rs::UTF_8);
                let (text, _, _) = encoding.decode(&bytes);
                Ok(text.to_string())
            },
            Body::InMemory(InMemoryBody::Text(s)) => Ok(s),
            Body::InMemory(InMemoryBody::Bytes(b)) => {
                String::from_utf8(b).map_err(|e| e.into())
            },
            Body::InMemory(InMemoryBody::Json(value)) => {
                let s = serde_json::to_string(&value)?;
                Ok(s)
            },
            Body::InMemory(InMemoryBody::Empty) => Ok(String::new()),
        }
    }

    pub async fn into_bytes(self) -> Result<Bytes> {
        match self {
            Body::Hyper(hyper_body) => {
                let bytes = hyper::body::to_bytes(hyper_body).await?;
                Ok(bytes)
            },
            Body::InMemory(InMemoryBody::Text(s)) => Ok(s.into_bytes().into()),
            Body::InMemory(InMemoryBody::Bytes(b)) => Ok(b.into()),
            Body::InMemory(InMemoryBody::Json(value)) => {
                let s = serde_json::to_string(&value)?;
                Ok(s.into_bytes().into())
            },
            Body::InMemory(InMemoryBody::Empty) => Ok(Bytes::new()),
        }
    }

    pub async fn into_json<D: DeserializeOwned>(self) -> Result<D> {
        match self {
            Body::Hyper(hyper_body) => {
                let bytes = hyper::body::to_bytes(hyper_body).await?;
                let encoding = Encoding::for_label(&[]).unwrap_or(encoding_rs::UTF_8);
                let (text, _, _) = encoding.decode(&bytes);
                serde_json::from_str(&text).map_err(|e| e.into())
            },
            Body::InMemory(InMemoryBody::Text(s)) => {
                serde_json::from_str(&s).map_err(|e| e.into())
            }
            Body::InMemory(InMemoryBody::Bytes(b)) => {
                let s = String::from_utf8(b)?;
                serde_json::from_str(&s).map_err(|e| e.into())
            }
            Body::InMemory(InMemoryBody::Json(value)) => {
                serde_json::from_value(value).map_err(|e| e.into())
            },
            Body::InMemory(InMemoryBody::Empty) => Err(crate::Error::JsonEncodingError(serde_json::Error::custom("empty body")))
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


impl HttpBody for Body {
    type Data = Bytes;
    type Error = crate::Error;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<std::result::Result<Self::Data, Self::Error>>> {
        let body = self.get_mut();
        match body {
            Body::InMemory(InMemoryBody::Empty) => Poll::Ready(None),
            Body::InMemory(InMemoryBody::Text(s)) => {
                if s.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = s.split_off(0);
                    Poll::Ready(Some(Ok(Bytes::from(data))))
                }
            }
            Body::InMemory(InMemoryBody::Bytes(b)) => {
                if b.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = b.split_off(0);
                    Poll::Ready(Some(Ok(Bytes::from(data))))
                }
            }
            Body::InMemory(InMemoryBody::Json(value)) => {
                let data = serde_json::to_vec(value)?;
                *body = Body::InMemory(InMemoryBody::Empty);
                Poll::Ready(Some(Ok(Bytes::from(data))))
            }
            Body::Hyper(body) => Pin::new(body).poll_data(cx).map_err(|e| e.into()),
        }
    }

    fn poll_trailers(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::result::Result<Option<HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }

    fn is_end_stream(&self) -> bool {
        use InMemoryBody::*;
        match self {
            Body::InMemory(Text(s)) => s.is_empty(),
            Body::InMemory(Empty) => true,
            Body::InMemory(Bytes(b)) => b.is_empty(),
            Body::InMemory(Json(_)) => false,
            Body::Hyper(b) => b.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        use InMemoryBody::*;
        match self {
            Body::InMemory(Text(s)) => SizeHint::with_exact(s.len() as u64),
            Body::InMemory(Empty) => SizeHint::with_exact(0),
            Body::InMemory(Bytes(b)) => SizeHint::with_exact(b.len() as u64),
            Body::InMemory(Json(_)) => SizeHint::default(),
            Body::Hyper(h) => h.size_hint(),
        }
    }
}