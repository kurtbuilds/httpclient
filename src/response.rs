use std::borrow::Cow;

use std::fmt;
use hyper::{StatusCode};
use encoding_rs::Encoding;
use hyper::body::Bytes;

use crate::body::{Body, NonStreamingBody};
use serde::{Serialize, Deserialize, Deserializer};
use serde::ser::SerializeMap;

use serde::de::{MapAccess};
use crate::headers::{AddHeaders, SortedHeaders};

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Response(
    #[serde(serialize_with = "serialize_response")]
    hyper::Response<Body>
);


impl Response {
    pub fn try_clone(&self) -> Result<Response, crate::Error> {
        let builder = hyper::Response::builder()
            .version(self.0.version())
            .headers(self.0.headers())
            .status(self.0.status());
        Ok(Response(builder.body(self.0.body().try_clone()?)?))
    }

    pub fn status(&self) -> StatusCode {
        self.0.status()
    }

    pub fn headers(&self) -> &hyper::HeaderMap {
        self.0.headers()
    }

    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.0.headers().get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                let cookies = basic_cookies::Cookie::parse(v).ok()?;
                cookies.into_iter().find(|c| c.get_name() == name).map(|c| c.get_value())
            })
    }

    pub fn body(&self) -> &Body {
        self.0.body()
    }

    pub fn body_mut(&mut self) -> &mut Body {
        self.0.body_mut()
    }

    pub fn error_for_status(self) -> Result<Self, Self> {
        let status = self.status();
        if status.is_server_error() || status.is_client_error() {
            Err(self)
        } else {
            Ok(self)
        }
    }

    pub fn error_for_status_ref(&self) -> Result<&Self, &Self> {
        let status = self.status();
        if status.is_server_error() || status.is_client_error() {
            Err(&self)
        } else {
            Ok(&self)
        }
    }

    pub async fn text(mut self) -> Result<String, crate::Error> {
        let bytes = hyper::body::to_bytes(self.0.body_mut()).await?;
        let encoding = Encoding::for_label(&[]).unwrap_or(encoding_rs::UTF_8);
        let (text, _, _) = encoding.decode(&bytes);
        Ok(text.to_string())
    }

    pub async fn json<U: serde::de::DeserializeOwned>(self) -> Result<U, crate::Error> {
        let text = self.text().await?;
        serde_json::from_str(&text).map_err(crate::Error::JsonError)
    }

    pub async fn bytes(mut self) -> Result<Bytes, crate::Error> {
        let bytes = hyper::body::to_bytes(self.0.body_mut()).await?;
        Ok(bytes)
    }

    pub fn into_parts(self) -> (http::response::Parts, Body) {
        self.0.into_parts()
    }

    pub fn from_parts(parts: http::response::Parts, body: Body) -> Self {
        Response(hyper::Response::from_parts(parts, body))
    }

    pub(crate) async fn into_infallible_cloneable(self) -> Result<Self, crate::Error> {
        let (parts, body) = self.into_parts();
        let content_type = parts.headers.get(hyper::header::CONTENT_TYPE);
        let body = body.into_memory(content_type).await?;
        Ok(Response::from_parts(parts, body))
    }
}


impl From<hyper::Response<hyper::Body>> for Response {
    fn from(response: hyper::Response<hyper::Body>) -> Self {
        let (parts, body) = response.into_parts();
        let body: Body = body.into();
        Response(hyper::Response::from_parts(parts, body))
    }
}


fn serialize_response<S>(value: &hyper::Response<Body>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
{
    let mut map = serializer.serialize_map(Some(3))?;
    map.serialize_entry("status", &value.status().as_u16())?;
    map.serialize_entry("headers", &SortedHeaders::from(value.headers()))?;
    map.serialize_entry("data", &NonStreamingBody::from(value.body()))?;
    map.end()
}


struct ResponseVisitor;

impl<'de> serde::de::Visitor<'de> for ResponseVisitor {
    type Value = Response;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("A map with the following keys: status, headers, data")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error> where A: MapAccess<'de> {
        let mut status = None;
        let mut headers = None;
        let mut data = None;
        while let Some(key) = map.next_key::<Cow<str>>()? {
            match key.as_ref() {
                "status" => {
                    if status.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("status"));
                    }
                    let i = map.next_value::<u16>()?;
                    status = Some(StatusCode::from_u16(i).map_err(|_e|
                        <A::Error as serde::de::Error>::custom("Invalid value for field `status`.")
                    )?);
                }
                "headers" => {
                    if headers.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("headers"));
                    }
                    headers = Some(map.next_value::<SortedHeaders>()?);
                }
                "data" => {
                    if data.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("body"));
                    }
                    data = Some(map.next_value::<NonStreamingBody>()?);
                }
                _ => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        let status = status.ok_or_else(|| serde::de::Error::missing_field("status"))?;
        let headers = headers.ok_or_else(|| serde::de::Error::missing_field("headers"))?;
        let data = data.ok_or_else(|| serde::de::Error::missing_field("data"))?;
        Ok(Response(hyper::Response::builder()
            .headers_from_sorted(headers)
            .status(status)
            .body(data.into())
            .unwrap()
        ))
    }
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
    {
        deserializer.deserialize_map(ResponseVisitor)
    }
}

pub struct ResponseWithBody<T> {
    pub data: T,
    pub headers: hyper::HeaderMap,
    pub status: StatusCode,
}

impl<T> ResponseWithBody<T> {
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.headers.get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                let cookies = basic_cookies::Cookie::parse(v).ok()?;
                cookies.into_iter().find(|c| c.get_name() == name).map(|c| c.get_value())
            })
    }
}