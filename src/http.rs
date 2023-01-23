use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};
use std::str::FromStr;
use http::{HeaderMap};
use http::header::HeaderName;
use crate::sanitize::{SANITIZED_VALUE, should_sanitize};


/// Only used for de/serialization. http::HeaderMap has a number of optimizations, so we want
/// to use it at runtime, but serialization needs to be in-order.
#[derive(Serialize, Deserialize, Default)]
#[serde(transparent)]
pub(crate) struct SortedSerializableHeaders(BTreeMap<String, String>);


impl From<&HeaderMap> for SortedSerializableHeaders {
    fn from(headers: &HeaderMap) -> Self {
        let mut map = BTreeMap::new();
        for (key, value) in headers.iter() {
            let key = key.as_str().to_string();
            let value = if should_sanitize(&key) {
                SANITIZED_VALUE.to_string()
            } else {
                value.to_str().unwrap().to_string()
            };
            map.insert(key, value);
        }
        SortedSerializableHeaders(map)
    }
}


impl Into<HeaderMap> for SortedSerializableHeaders {
    fn into(self) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (key, value) in self.0.into_iter() {
            map.insert(HeaderName::from_str(key.as_str()).unwrap(), value.parse().unwrap());
        }
        map
    }
}
