use http::header::CONTENT_TYPE;
use crate::{InMemoryResponse, InMemoryResponseExt, multipart};
use crate::multipart::part::Part;
use crate::multipart::{write_boundary, write_headers, write_terminate, WriteBytes};

/// Form<B> does not have headers. This is an intentional design decision, because
/// if you have a request body that's multipart, you have a Request<Form<B>>, and the request
/// already has headers. Therefore, Form<B> not having its own headers makes this more composable.
///
/// If you need headers, use Part<Form<B>>
#[derive(Debug)]
pub struct Form<B> {
    pub boundary: String,
    // doesn't yet include the boundary. use `full_content_type` to get the full content type.
    pub content_type: String,
    pub parts: Vec<Part<B>>,
}

impl Form<InMemoryResponse> {
    pub fn from_response(res: InMemoryResponse) -> Option<Self> {
        let header = res.headers().get(CONTENT_TYPE)?;
        let header = header.to_str().ok()?;
        let (content, boundary) = header.split_once("; boundary=")?;
        let mut form = Form {
            content_type: content.to_string(),
            boundary: boundary.to_string(),
            parts: Vec::new(),
        };
        let boundary = format!("--{}", boundary);
        let text = res.text().ok()?;
        let mut splits = text.split(&boundary).skip(1);
        while let Some(mut part) = splits.next() {
            if part.starts_with("--\r\n") {
                break;
            }
            debug_assert!(part.starts_with("\r\n"));
            part = &part[2..];
            let (headers, mut part) = multipart::parse_headers(part)?;
            debug_assert!(part.starts_with("\r\n"));
            part = &part[2..];
            let body = multipart::parse_response(part)?;
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

    pub fn mixed() -> Self {
        Form {
            content_type: "multipart/mixed".to_string(),
            boundary: multipart::gen_boundary(),
            parts: Vec::new(),
        }
    }

    pub fn alternative() -> Self {
        Form {
            content_type: "multipart/alternative".to_string(),
            boundary: multipart::gen_boundary(),
            parts: Vec::new(),
        }
    }

    pub fn form_data() -> Self {
        Form {
            content_type: "multipart/form-data".to_string(),
            boundary: multipart::gen_boundary(),
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

impl<T: WriteBytes> From<Form<T>> for Vec<u8> {
    fn from(value: Form<T>) -> Self {
        let boundary = value.boundary.as_bytes();
        let mut buf = Vec::new();
        for part in value.parts {
            write_boundary(&mut buf, boundary);
            write_headers(&mut buf, &part.headers);
            let n = buf.len();
            part.body.write(&mut buf);
            if buf.len() > n {
                buf.extend_from_slice(b"\r\n");
            }
        }
        write_terminate(&mut buf, boundary);
        buf
    }
}
