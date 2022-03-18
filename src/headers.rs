use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fmt::Write;
use std::str::FromStr;
use http::header::HeaderName;
use http::HeaderMap;
use serde::de::{EnumAccess, Error, SeqAccess};
use serde::{Serialize, Deserialize};


pub trait AddHeaders {
    fn headers(self, headers: &http::HeaderMap) -> Self;
    fn headers_from_sorted(self, headers: SortedHeaders) -> Self;
}

impl AddHeaders for http::request::Builder {
    fn headers(mut self, headers: &HeaderMap) -> Self {
        for (key, value) in headers.iter() {
            self = self.header(key, value);
        }
        self
    }

    fn headers_from_sorted(mut self, headers: SortedHeaders) -> Self {
        for (key, value) in headers.iter() {
            self = self.header(key, value);
        }
        self
    }
}

impl AddHeaders for http::response::Builder {
    fn headers(mut self, headers: &HeaderMap) -> Self {
        for (key, value) in headers.iter() {
            self = self.header(key, value);
        }
        self
    }

    fn headers_from_sorted(mut self, headers: SortedHeaders) -> Self {
        for (key, value) in headers.iter() {
            self = self.header(key, value);
        }
        self
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct SortedHeaders(BTreeMap<String, String>);

impl SortedHeaders {
    fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

impl From<&http::header::HeaderMap> for SortedHeaders {
    fn from(headers: &http::header::HeaderMap) -> Self {
        let mut map = BTreeMap::new();
        for (key, value) in headers.iter() {
            map.insert(key.to_string(), value.to_str().unwrap().to_string());
        }
        SortedHeaders(map)
    }
}


impl Into<http::header::HeaderMap> for SortedHeaders {
    fn into(self) -> http::header::HeaderMap {
        let mut map = http::header::HeaderMap::new();
        for (key, value) in self.0.into_iter() {
            map.insert(HeaderName::from_str(key.as_str()).unwrap(), value.parse().unwrap());
        }
        map
    }
}