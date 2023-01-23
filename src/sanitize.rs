use http::{HeaderMap, HeaderValue};
use once_cell::sync::OnceCell;
use regex::Regex;
use serde_json::Value;

static REGEX: OnceCell<Regex> = OnceCell::new();

fn regex() -> &'static Regex {
    REGEX.get_or_init(|| {
        let s = [
            "secret",
            "key",
            "pkey",
            "session",
            "password"
        ].map(|s| format!(r#"(\b|[-_]){s}(\b|[-_])"#)).join("|");
        Regex::new(&format!(r#"(?i)({s})"#)).unwrap()
    })
}

pub static SANITIZED_VALUE: &str = "**********";

pub fn should_sanitize(key: &str) -> bool {
    match key {
        "authorization" => true,
        "cookie" => true,
        "set-cookie" => true,
        "password" => true,
        _ if regex().is_match(key) => true,
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
    let sanitized: HeaderValue = SANITIZED_VALUE.parse().unwrap();
    for (key, value) in headers.iter_mut() {
        if should_sanitize(key.as_str()) {
            *value = sanitized.clone();
        }
    }
}