use http::header::IntoHeaderName;
use http::{HeaderName, HeaderValue, Uri};

pub use builder::RequestBuilder;
pub use memory::*;

use crate::Body;

mod builder;
mod memory;

pub type Request<T = Body> = http::Request<T>;

pub trait RequestExt {
    fn host(&self) -> &str;
    fn path(&self) -> &str;
    fn url(&self) -> &Uri;
    fn header<H: TryInto<HeaderName>>(&self, h: H) -> Option<&HeaderValue>;
}

impl<B> RequestExt for Request<B> {
    fn host(&self) -> &str {
        self.uri().host().unwrap_or_default()
    }

    fn path(&self) -> &str {
        self.uri().path()
    }

    fn url(&self) -> &Uri {
        self.uri()
    }

    fn header<H: TryInto<HeaderName>>(&self, h: H) -> Option<&HeaderValue> {
        let h = h.try_into().ok()?;
        self.headers().get(h)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::{Client, InMemoryBody};

    use super::*;

    #[test]
    fn test_push_query() {
        let mut r1 = RequestBuilder::get("https://example.com/foo/bar");
        r1 = r1.query("a", "b");

        assert_eq!(r1.uri.to_string(), "https://example.com/foo/bar?a=b");
        r1 = r1.query("c", "d");
        assert_eq!(r1.uri.to_string(), "https://example.com/foo/bar?a=b&c=d");
    }

    #[test]
    fn test_query() {
        let r1 = RequestBuilder::get("http://example.com/foo/bar").set_query(HashMap::from([("a", Some("b")), ("c", Some("d")), ("e", None)]));
        let r1 = r1.body(InMemoryBody::Empty);
        let value: HashMap<String, String> = serde_qs::from_str(r1.uri.query().unwrap()).unwrap();
        assert_eq!(value.get("a"), Some(&"b".to_string()));
        assert_eq!(value.get("c"), Some(&"d".to_string()));
        assert_eq!(value.len(), 2);
        assert!(r1.uri.to_string().starts_with("http://example.com/foo/bar?"));
    }

    #[test]
    fn test_client_request() {
        let client = Client::new();
        let _ = client.post("/foo").json(json!({"a": 1}));
    }
}
