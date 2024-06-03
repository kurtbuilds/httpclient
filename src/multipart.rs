use http::HeaderMap;
use rand::Rng;

use crate::InMemoryBody;

fn gen_boundary() -> String {
    let mut rng = rand::thread_rng();

    let a = rng.gen::<u64>();
    let b = rng.gen::<u64>();
    let c = rng.gen::<u64>();
    let d = rng.gen::<u64>();

    format!("{a:016x}-{b:016x}-{c:016x}-{d:016x}")
}

pub struct Form {
    pub boundary: String,
    // doesn't yet include the boundary. use `full_content_type` to get the full content type.
    pub content_type: String,
    pub parts: Vec<Part>,
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Form {
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
            content_type: "multipart/form-data".to_string(),
            boundary,
            parts: Vec::new(),
        }
    }

    #[must_use]
    pub fn part(mut self, part: Part) -> Self {
        self.parts.push(part);
        self
    }
}

impl From<Form> for Vec<u8> {
    fn from(val: Form) -> Self {
        let mut bytes = Vec::new();
        for part in val.parts {
            bytes.extend_from_slice("--".as_bytes());
            bytes.extend_from_slice(val.boundary.as_bytes());
            bytes.extend_from_slice("\r\n".as_bytes());
            for (key, value) in &part.headers {
                let key = key.as_str();
                bytes.extend_from_slice(key.as_bytes());
                bytes.extend_from_slice(": ".as_bytes());
                bytes.extend_from_slice(value.as_bytes());
                bytes.extend_from_slice("\r\n".as_bytes());
            }
            bytes.extend_from_slice("\r\n".as_bytes());
            let body = part.body.bytes().expect("Failed to convert body to bytes");
            bytes.extend_from_slice(body.as_ref());
            bytes.extend_from_slice("\r\n".as_bytes());
        }
        bytes.extend_from_slice("--".as_bytes());
        bytes.extend_from_slice(val.boundary.as_bytes());
        bytes.extend_from_slice("--\r\n".as_bytes());
        bytes
    }
}

pub struct Part {
    pub headers: HeaderMap,
    pub body: InMemoryBody,
}

impl Part {
    #[must_use]
    pub fn new(body: InMemoryBody) -> Self {
        Part { headers: HeaderMap::new(), body }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_bytes() {
        let mut form = Form::new();
        let mut part = Part::new(InMemoryBody::Text("GET /farm/v1/animals/pony".to_string()));

        part.headers
            .insert(http::header::CONTENT_TYPE, "application/http".parse().expect("Unable to parse content type"));

        form.parts.push(part);

        let boundary = form.boundary.clone();
        let bytes: Vec<u8> = form.into();
        let s = String::from_utf8(bytes).expect("Unable to convert bytes to string");
        let right = format!("--{0}\r\ncontent-type: application/http\r\n\r\nGET /farm/v1/animals/pony\r\n--{0}--\r\n", &boundary);
        assert_eq!(s, right);
    }
}
