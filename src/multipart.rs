use http::{header, HeaderMap, StatusCode};
use rand::Rng;
use std::str::FromStr;
use http::header::AsHeaderName;

use crate::{InMemoryBody, InMemoryRequest, InMemoryResponse, InMemoryResponseExt};

fn gen_boundary() -> String {
    let mut rng = rand::thread_rng();

    let a = rng.gen::<u64>();
    let b = rng.gen::<u64>();
    let c = rng.gen::<u64>();
    let d = rng.gen::<u64>();

    format!("{a:016x}-{b:016x}-{c:016x}-{d:016x}")
}

fn parse_headers(mut text: &str) -> Option<(HeaderMap, &str)> {
    let mut headers = HeaderMap::new();
    while let Some((line, rest)) = text.split_once("\r\n") {
        let Some((header, value)) = line.split_once(": ") else {
            break;
        };
        let header = header::HeaderName::from_str(header).ok()?;
        let value = value.to_string();
        headers.insert(header, value.parse().ok()?);
        text = rest;
    }
    Some((headers, text))
}

/// parse a response from a string
/// formed like
/// HTTP/1.1 200 OK
/// Content-Type: application/json
/// Content-Length: 2
/// <body>
fn parse_response(text: &str) -> Option<InMemoryResponse> {
    let (line, text) = text.split_once("\r\n")?;
    let mut split = line.splitn(3, " ");
    let _version = split.next()?;
    let status = split.next()?;
    let status = StatusCode::from_str(status).ok()?;
    let _reason = split.next()?;

    let (headers, text) = parse_headers(text)?;
    let body = InMemoryBody::Text(text.to_string());
    let mut res = http::Response::builder().status(status);
    *res.headers_mut().unwrap() = headers;
    res.body(body).ok()
}

#[derive(Debug)]
pub struct Form<B> {
    pub boundary: String,
    // doesn't yet include the boundary. use `full_content_type` to get the full content type.
    pub content_type: String,
    pub parts: Vec<Part<B>>,
}

impl Default for Form<InMemoryBody> {
    fn default() -> Self {
        Self::new()
    }
}

impl Form<InMemoryResponse> {
    pub fn from_response(res: InMemoryResponse) -> Option<Self> {
        let mut form = Form::new();
        let header = res.headers().get(header::CONTENT_TYPE)?;
        let header = header.to_str().ok()?;
        let boundary = header.split_once("boundary=")?.1;
        let boundary = format!("--{}", boundary);
        let text = res.text().ok()?;
        let mut splits = text.split(&boundary).skip(1);
        while let Some(mut part) = splits.next() {
            if part.starts_with("--\r\n") {
                break;
            }
            debug_assert!(part.starts_with("\r\n"));
            part = &part[2..];
            let (headers, mut part) = parse_headers(part)?;
            debug_assert!(part.starts_with("\r\n"));
            part = &part[2..];
            let body = parse_response(part)?;
            form.push(Part { headers, body });
        }
        Some(form)
    }
}

impl<B> Form<B> {
    #[must_use]
    pub fn full_content_type(&self) -> String {
        format!("{}; boundary={}", self.content_type, &self.boundary)
    }

    #[must_use]
    pub fn content_type(mut self, content_type: String) -> Self {
        self.content_type = content_type;
        self
    }

    #[must_use]
    pub fn boundary(mut self, boundary: String) -> Self {
        self.boundary = boundary;
        self
    }

    #[must_use]
    pub fn new() -> Self {
        let boundary = gen_boundary();
        Form {
            content_type: "multipart/mixed".to_string(),
            boundary,
            parts: Vec::new(),
        }
    }

    #[must_use]
    pub fn part(mut self, part: Part<B>) -> Self {
        self.parts.push(part);
        self
    }

    pub fn push(&mut self, part: Part<B>) {
        self.parts.push(part);
    }
}

fn write_terminate(buf: &mut Vec<u8>, boundary: &[u8]) {
    buf.extend_from_slice(b"--");
    buf.extend_from_slice(boundary);
    buf.extend_from_slice(b"--\r\n");
}

fn write_boundary(buf: &mut Vec<u8>, boundary: &[u8]) {
    buf.extend_from_slice(b"--");
    buf.extend_from_slice(boundary);
    buf.extend_from_slice(b"\r\n");
}

fn write_headers(buf: &mut Vec<u8>, headers: &HeaderMap) {
    for (key, value) in headers {
        let key = key.as_str();
        buf.extend_from_slice(key.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }
    buf.extend_from_slice(b"\r\n");
}

/// trait to define how to write bytes into a request buffer
pub trait WriteBytes {
    fn write(self, buf: &mut Vec<u8>);
}

impl<T: WriteBytes> From<Form<T>> for Vec<u8> {
    fn from(value: Form<T>) -> Self {
        let boundary = value.boundary.as_bytes();
        let mut buf = Vec::new();
        for part in value.parts {
            write_boundary(&mut buf, boundary);
            write_headers(&mut buf, &part.headers);
            part.body.write(&mut buf);
        }
        write_terminate(&mut buf, boundary);
        buf
    }
}

impl WriteBytes for InMemoryRequest {
    fn write(self, buf: &mut Vec<u8>) {
        let method = self.method().as_str();
        let uri = self.uri().path();
        buf.extend_from_slice(method.as_bytes());
        buf.extend(b" ");
        buf.extend_from_slice(uri.as_bytes());
        buf.extend_from_slice(b"\r\n");
        let body = self.into_body();
        body.write(buf);
    }
}

impl WriteBytes for Vec<u8> {
    fn write(self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self);
    }
}

impl WriteBytes for InMemoryBody {
    fn write(self, buf: &mut Vec<u8>) {
        match self {
            InMemoryBody::Empty => {}
            InMemoryBody::Bytes(b) => buf.extend_from_slice(&b),
            InMemoryBody::Text(s) => buf.extend_from_slice(s.as_bytes()),
            InMemoryBody::Json(val) => {
                let content = serde_json::to_string(&val).expect("Failed to convert json to string");
                buf.extend_from_slice(content.as_bytes());
            }
        }
    }
}

#[derive(Debug)]
pub struct Part<B> {
    pub headers: HeaderMap,
    pub body: B,
}

impl<B> Part<B> {
    #[must_use]
    pub fn content_id(mut self, id: &str) -> Self {
        self.headers.insert("Content-ID", id.parse().expect("Unable to parse content id"));
        self
    }

    pub fn header_str<H: AsHeaderName>(&self, h: H) -> Option<&str> {
        self.headers.get(h).and_then(|v| v.to_str().ok())
    }
}

impl Part<InMemoryRequest> {
    #[must_use]
    pub fn new(body: InMemoryRequest) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::CONTENT_TYPE, "application/http".parse().expect("Unable to parse content type"));
        Part { headers, body }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;

    #[test]
    fn test_to_bytes() {
        let mut form = Form::new();
        let part = Part::new(Request::builder().uri("/farm/v1/animals/pony").body(InMemoryBody::Empty).unwrap());
        form.parts.push(part);

        let boundary = form.boundary.clone();
        let bytes: Vec<u8> = form.into();
        let s = String::from_utf8(bytes).expect("Unable to convert bytes to string");
        let right = format!("--{0}\r\ncontent-type: application/http\r\n\r\nGET /farm/v1/animals/pony\r\n--{0}--\r\n", &boundary);
        assert_eq!(s, right);
    }
}
