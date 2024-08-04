use http::{header, HeaderMap, HeaderValue};
use http::header::{AsHeaderName, CONTENT_TYPE, IntoHeaderName};
use crate::{InMemoryBody, InMemoryRequest, multipart};
use crate::multipart::WriteBytes;
use crate::multipart::form::Form;

impl<T: WriteBytes> WriteBytes for Part<T> {
    fn write(self, buf: &mut Vec<u8>) {
        multipart::write_headers(buf, &self.headers);
        self.body.write(buf);
    }
}

#[derive(Debug)]
pub struct Part<B> {
    pub headers: HeaderMap,
    pub body: B,
}

impl<B> Part<B> {
    pub fn new(headers: HeaderMap, body: B) -> Self {
        Part { headers, body }
    }

    #[must_use]
    pub fn content_id(mut self, id: &str) -> Self {
        self.headers.insert("Content-ID", id.parse().expect("Unable to parse content id"));
        self
    }

    pub fn header_str<H: AsHeaderName>(&self, h: H) -> Option<&str> {
        self.headers.get(h).and_then(|v| v.to_str().ok())
    }

    pub fn header<H: IntoHeaderName>(mut self, h: H, v: HeaderValue) -> Self {
        self.headers.insert(h, v);
        self
    }

}

impl Part<InMemoryRequest> {
    pub fn request(body: InMemoryRequest) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/http".parse().expect("Unable to parse content type"));
        Part { headers, body }
    }
}

impl Part<InMemoryBody> {
    pub fn text(body: String) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "text/plain".parse().expect("Unable to parse content type"));
        Part { headers, body: InMemoryBody::Text(body) }
    }

    pub fn html(body: String) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "text/html".parse().expect("Unable to parse content type"));
        Part { headers, body: InMemoryBody::Text(body) }
    }

    pub fn form(form: Form<InMemoryBody>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, form.full_content_type().parse().expect("Unable to parse content type"));
        let body: Vec<u8> = form.into();
        let body = match String::from_utf8(body) {
            Ok(s) => InMemoryBody::Text(s),
            Err(e) => {
                InMemoryBody::Bytes(e.into_bytes())
            }
        };
        Part { headers, body }
    }
}

impl<T: Default> Default for Part<T> {
    fn default() -> Self {
        Part::new(HeaderMap::new(), T::default())
    }
}

impl<T: WriteBytes> From<Part<T>> for Vec<u8> {
    fn from(value: Part<T>) -> Self {
        let mut buf = Vec::new();
        value.write(&mut buf);
        buf
    }
}

impl Into<InMemoryBody> for Part<InMemoryBody> {
    fn into(self) -> InMemoryBody {
        let mut buf = Vec::new();
        self.write(&mut buf);
        match String::from_utf8(buf) {
            Ok(s) => InMemoryBody::Text(s),
            Err(e) => InMemoryBody::Bytes(e.into_bytes())
        }
    }
}
