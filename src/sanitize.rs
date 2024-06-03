use http::{HeaderMap, HeaderValue};
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

static REGEX: OnceLock<Regex> = OnceLock::new();

trait AsLowercase {
    fn as_lowercase(&self) -> std::borrow::Cow<str>;
}

impl AsLowercase for str {
    fn as_lowercase(&self) -> std::borrow::Cow<str> {
        use std::borrow::Cow;
        if let Some(first_uppercase) = self.bytes().position(|b| b.is_ascii_alphabetic() && !b.is_ascii_lowercase()) {
            let mut string = String::with_capacity(self.len());
            string.push_str(&self[..first_uppercase]);
            for b in self[first_uppercase..].chars() {
                string.push(b.to_ascii_lowercase());
            }
            Cow::Owned(string)
        } else {
            Cow::Borrowed(self)
        }
    }
}

fn regex() -> &'static Regex {
    REGEX.get_or_init(|| {
        let s = ["secret", "key", "pkey", "session", "password", "token"]
            .map(|s| format!(r#"(\b|[-_]){s}(\b|[-_])"#))
            .join("|");

        Regex::new(&format!(r#"(?i)({s})"#)).expect("Unable to compile regex")
    })
}

pub static SANITIZED_VALUE: &str = "**********";

pub fn should_sanitize(key: &str) -> bool {
    let key = key.as_lowercase();
    match key.as_ref() {
        "authorization" | "cookie" | "password" | "set-cookie" => true,
        _ if regex().is_match(key.as_ref()) => true,
        _ => false,
    }
}

pub fn sanitize_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if should_sanitize(key) {
                    *value = Value::String(SANITIZED_VALUE.to_string());
                } else {
                    sanitize_value(value);
                }
            }
        }
        Value::Array(vec) => {
            for value in vec.iter_mut() {
                sanitize_value(value);
            }
        }
        _ => {}
    }
}

pub fn sanitize_headers(headers: &mut HeaderMap) {
    let sanitized: HeaderValue = SANITIZED_VALUE.parse().expect("Unable to parse sanitized value");
    for (key, value) in headers.iter_mut() {
        if should_sanitize(key.as_str()) {
            *value = sanitized.clone();
        }
    }
}
