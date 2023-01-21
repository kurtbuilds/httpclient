
use std::hash::Hasher;
use std::pin::Pin;

use std::task::{Context, Poll};
use encoding_rs::Encoding;
use http::{HeaderMap, HeaderValue};
use hyper::body::{Bytes, HttpBody, SizeHint};
use serde::{Serialize, Deserialize};
use serde_json::Value;


#[derive(Debug)]
pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    Text(String),
    Hyper(hyper::Body),
    Json(Value),
}


impl Body {
    pub fn is_empty(&self) -> bool {
        match self {
            Body::Empty => true,
            Body::Bytes(b) => b.is_empty(),
            Body::Text(s) => s.is_empty(),
            Body::Hyper(b) => b.size_hint().upper() == Some(0),
            Body::Json(_) => false,
        }
    }

    pub async fn into_memory(self, content_type: Option<&HeaderValue>) -> Result<Self, crate::Error> {
        match self {
            Body::Empty | Body::Bytes(_) | Body::Text(_) | Body::Json(_) => Ok(self),
            Body::Hyper(hyper_body) => {
                let bytes = hyper::body::to_bytes(hyper_body).await?;
                let encoding = Encoding::for_label(&[]).unwrap_or(encoding_rs::UTF_8);
                let (text, _, _) = encoding.decode(&bytes);
                let content_type = content_type.map(|ct| ct.to_str().unwrap().split(';').next().unwrap());
                match content_type {
                    Some("application/json") => Ok(Body::Json(serde_json::from_str::<Value>(text.as_ref())?)),
                    _ => Ok(Body::Text(text.to_string())),
                }
            }
        }
    }

    pub fn try_clone(&self) -> Result<Self, crate::Error> {
        match self {
            Body::Empty => Ok(Body::Empty),
            Body::Bytes(b) => Ok(Body::Bytes(b.clone())),
            Body::Text(s) => Ok(Body::Text(s.clone())),
            Body::Hyper(_b) => Err(crate::Error::Generic(
                "hyper::Body cannot be cloned".to_string()
            )),
            Body::Json(v) => Ok(Body::Json(v.clone())),
        }
    }
}

impl From<hyper::Body> for Body {
    fn from(body: hyper::Body) -> Self {
        Body::Hyper(body)
    }
}


impl Into<hyper::Body> for Body {
    fn into(self) -> hyper::Body {
        match self {
            Body::Empty => hyper::Body::empty(),
            Body::Bytes(bytes) => hyper::Body::from(bytes),
            Body::Text(text) => hyper::Body::from(text),
            Body::Hyper(body) => body,
            Body::Json(ref value) => hyper::Body::from(serde_json::to_string(value).unwrap()),
        }
    }
}


impl std::hash::Hash for Body {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Body::Empty => {}
            Body::Bytes(b) => b.hash(state),
            Body::Text(s) => s.hash(state),
            Body::Hyper(_) => panic!("Hyper body cannot be hashed."),
            Body::Json(ref v) => {
                let body = serde_json::to_string(&v).unwrap();
                body.hash(state)
            },
        }
    }
}


impl hyper::body::HttpBody for Body {
    type Data = hyper::body::Bytes;
    type Error = crate::Error;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let z = self.get_mut();
        if let Body::Json(value) = z {
            let data = serde_json::to_vec(value)?;
            *z = Body::Empty;
            return Poll::Ready(Some(Ok(Bytes::from(data))));
        }
        match z {
            Body::Empty => Poll::Ready(None),
            Body::Bytes(b) => {
                if b.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = b.split_off(0);
                    Poll::Ready(Some(Ok(Bytes::from(data))))
                }
            }
            Body::Text(s) => {
                if s.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = s.split_off(0);
                    Poll::Ready(Some(Ok(Bytes::from(data))))
                }
            }
            Body::Hyper(h) => Pin::new(h).poll_data(cx).map_err(|e| e.into()),
            Body::Json(_) => {
                // handled above. can't seem to do it in the match because we need to modify the
                // reference to z.
                unreachable!();
            }
        }
    }

    fn poll_trailers(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Body::Empty => true,
            Body::Bytes(b) => b.is_empty(),
            Body::Text(s) => s.is_empty(),
            Body::Hyper(body) => body.is_end_stream(),
            Body::Json(ref _v) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Body::Empty => SizeHint::with_exact(0),
            Body::Bytes(b) => SizeHint::with_exact(b.len() as u64),
            Body::Text(s) => SizeHint::with_exact(s.bytes().len() as u64),
            Body::Hyper(h) => h.size_hint(),
            Body::Json(_) => SizeHint::default(), // no easy way to get the size without just creating the body outright.
        }
    }
}


#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NonStreamingBody {
    Empty,
    Object(Value),
    String(String),
    Bytes(Vec<u8>),
}


impl From<&Body> for NonStreamingBody {
    fn from(body: &Body) -> Self {
        match body {
            Body::Empty => NonStreamingBody::Empty,
            Body::Bytes(b) => NonStreamingBody::Bytes(b.clone()),
            Body::Text(s) => NonStreamingBody::String(s.clone()),
            Body::Hyper(_body) => panic!("Hyper body cannot be converted to BodyContent"),
            Body::Json(ref v) => NonStreamingBody::Object(v.clone()),
        }
    }
}


impl Into<Body> for NonStreamingBody {
    fn into(self) -> Body {
        match self {
            NonStreamingBody::Empty => Body::Empty,
            NonStreamingBody::String(s) => Body::Text(s),
            NonStreamingBody::Object(v) => match &v {
                Value::Null => Body::Text("null".to_string()),
                Value::String(s) => Body::Text(s.clone()),
                Value::Bool(b) => Body::Text(b.to_string()),
                Value::Number(v) => Body::Text(v.to_string()),
                Value::Array(_a) => Body::Text(v.to_string()),
                Value::Object(_o) => Body::Json(v)
            },
            NonStreamingBody::Bytes(b) => Body::Bytes(b),
        }
    }
}