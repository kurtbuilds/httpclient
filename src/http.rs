use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};
use std::str::FromStr;
use http::{HeaderMap, Method, StatusCode, Uri};
use http::header::HeaderName;


#[derive(Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct SortedSerializableHeaders(BTreeMap<String, String>);

impl SortedSerializableHeaders {
    fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

impl From<&HeaderMap> for SortedSerializableHeaders {
    fn from(headers: &http::header::HeaderMap) -> Self {
        let mut map = BTreeMap::new();
        for (key, value) in headers.iter() {
            map.insert(key.to_string(), value.to_str().unwrap().to_string());
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