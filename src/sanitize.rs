use once_cell::sync::OnceCell;
use regex::Regex;
use serde_json::Value;

static REGEX: OnceCell<Regex> = OnceCell::new();

fn regex() -> &'static Regex {
    REGEX.get_or_init(|| {
        Regex::new(r#"(?i)((\b|[-_])secret(\b|[-_])|(\b|[-_])key(\b|[-_])|(\b|[-_])pkey(\b|[-_])|(\b|[-_])session(\b|[-_])|(\b|[-_])sessid(\b|[-_]))"#).unwrap()
    })
}

pub static SANITIZED_VALUE: &str = "**********";

pub fn should_sanitize(key: &str) -> bool {
    match key {
        "Authorization" => true,
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