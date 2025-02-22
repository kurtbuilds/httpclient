use crate::{InMemoryBody, InMemoryRequest, InMemoryResponse};
pub use form::Form;
use http::{header, HeaderMap, StatusCode};
pub use part::Part;
use rand::Rng;
use std::str::FromStr;

mod form;
mod part;

fn gen_boundary() -> String {
    #[cfg(all(debug_assertions, feature = "mock"))]
    if let Some(boundary) = mock::BOUNDARY.lock().unwrap().as_ref() {
        return boundary.clone();
    }

    let mut rng = rand::rng();

    let a = rng.random::<u64>();
    let b = rng.random::<u64>();
    let c = rng.random::<u64>();
    let d = rng.random::<u64>();

    format!("{a:016x}-{b:016x}-{c:016x}-{d:016x}")
}

#[cfg(feature = "mock")]
pub mod mock {
    use super::*;

    pub(crate) static BOUNDARY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    pub fn set(s: String) {
        *BOUNDARY.lock().unwrap() = Some(s);
    }

    pub fn clear() {
        *BOUNDARY.lock().unwrap() = None;
    }

    pub struct BoundaryGuard;

    impl Drop for BoundaryGuard {
        fn drop(&mut self) {
            clear();
        }
    }

    pub fn scope(s: String) -> BoundaryGuard {
        set(s);
        BoundaryGuard
    }
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

impl WriteBytes for InMemoryRequest {
    fn write(self, buf: &mut Vec<u8>) {
        let method = self.method().as_str();
        let uri = self.uri().path();
        buf.extend_from_slice(method.as_bytes());
        buf.extend(b" ");
        buf.extend_from_slice(uri.as_bytes());
        let body = self.into_body();
        if !body.is_empty() {
            buf.extend_from_slice(b"\r\n");
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use serde_json::json;

    #[test]
    fn test_to_bytes() {
        let boundary = "zzz".to_string();
        let mut form = Form {
            content_type: "multipart/mixed".to_string(),
            boundary: boundary.clone(),
            parts: Vec::new(),
        };
        let part = Part::request(Request::builder().uri("/farm/v1/animals/pony").body(InMemoryBody::Empty).unwrap());
        form.parts.push(part);

        let bytes: Vec<u8> = form.into();
        let s = String::from_utf8(bytes).expect("Unable to convert bytes to string");
        let right = format!("--{0}\r\ncontent-type: application/http\r\n\r\nGET /farm/v1/animals/pony\r\n--{0}--\r\n", &boundary);
        assert_eq!(s, right);
    }

    #[test]
    fn test_to_bytes2() {
        let boundary = "zzz".to_string();
        let mut form = Form {
            content_type: "multipart/mixed".to_string(),
            boundary: boundary.clone(),
            parts: Vec::new(),
        };
        let mut headers = HeaderMap::new();
        headers.insert("Content-Disposition", "form-data; name=\"MetaData\"".parse().unwrap());
        let part = Part::new(
            headers,
            InMemoryBody::Json(json!({
                "TransactionId": 1,
                "Content": "message",
                "DisputeTypeCode": "BackupRequest",
                "DisputeTypeDescription": "Backup Request",
                "Documents": []
            })),
        );
        form.parts.push(part);
        let bytes: Vec<u8> = form.into();
        let s = String::from_utf8(bytes).expect("Unable to convert bytes to string");
        let right = "--zzz\r\ncontent-disposition: form-data; name=\"MetaData\"\r\n\r\n{\"Content\":\"message\",\"DisputeTypeCode\":\"BackupRequest\",\"DisputeTypeDescription\":\"Backup Request\",\"Documents\":[],\"TransactionId\":1}\r\n--zzz--\r\n";
        assert_eq!(s, right);
    }
}
