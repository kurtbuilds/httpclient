use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use walkdir::WalkDir;

use crate::error::ProtocolResult;
use crate::request::RequestExt;
use crate::sanitize::{sanitize_request, sanitize_response};
use crate::{InMemoryBody, InMemoryRequest, InMemoryResponse};

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestResponsePair {
    #[serde(with = "crate::request::serde_request")]
    pub request: InMemoryRequest,
    #[serde(with = "crate::response::serde_response")]
    pub response: InMemoryResponse,
}

#[derive(Debug)]
pub struct Recording {
    pub request: InMemoryRequest,
    pub response: InMemoryResponse,
    pub filename: String,
}

pub struct HashableRequest(pub InMemoryRequest);

impl std::fmt::Debug for HashableRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Deref for HashableRequest {
    type Target = InMemoryRequest;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Hash for HashableRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // method
        self.method().hash(state);
        // url, contains query params.
        self.uri().hash(state);
        // headers, sorted
        // let mut sorted = self.headers().iter()
        //     .map(|(k, v)| (k.as_str(), v.as_bytes()))
        //     .collect::<Vec<(&str, &[u8])>>();
        // sorted.sort();
        // sorted.into_iter().for_each(|(k, v)| {
        //     k.hash(state);
        //     v.hash(state);
        // });
        // body
        self.body().hash(state);
    }
}

impl PartialEq for HashableRequest {
    fn eq(&self, other: &Self) -> bool {
        if !(self.method() == other.method() && self.uri() == other.uri()) {
            return false;
        }
        match (self.body(), other.body()) {
            (InMemoryBody::Empty, InMemoryBody::Empty) => true,
            (InMemoryBody::Text(ref a), InMemoryBody::Text(ref b)) => a == b,
            (InMemoryBody::Bytes(ref a), InMemoryBody::Bytes(ref b)) => a == b,
            (InMemoryBody::Json(ref a), InMemoryBody::Json(ref b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for HashableRequest {}

#[derive(Debug, Clone)]
pub struct RequestRecorder {
    pub base_path: PathBuf,
    pub requests: Arc<RwLock<IndexMap<HashableRequest, InMemoryResponse>>>,
}

fn load_requests(path: &PathBuf) -> impl Iterator<Item = Recording> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name().to_str().unwrap().ends_with(".json"))
        .map(|filepath| {
            debug!(file = filepath.path().display().to_string(), "Loading recording");
            let f = fs::read_to_string(filepath.path()).unwrap();
            let rr: RequestResponsePair = serde_json::from_str(&f).unwrap();
            Recording {
                request: rr.request,
                response: rr.response,
                filename: filepath.path().file_name().unwrap().to_str().unwrap().to_string(),
            }
        })
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

impl RequestRecorder {
    pub fn new() -> Self {
        let path = std::env::current_dir().unwrap().join("data").join("vcr");
        debug!(dir = path.display().to_string(), "Request recorder created");
        let mut requests = load_requests(&path).collect::<Vec<_>>();
        requests.sort_by_key(|rr| rr.filename.clone());
        let requests: IndexMap<HashableRequest, InMemoryResponse> = requests.into_iter().map(|r| (HashableRequest(r.request), r.response)).collect::<_>();
        info!(num_recordings = requests.len(), dir = path.display().to_string(), "Request recorder loaded");
        let requests = Arc::new(RwLock::new(requests));
        RequestRecorder { base_path: path, requests }
    }

    pub fn get_response(&self, request: &HashableRequest) -> Option<InMemoryResponse> {
        debug!(url = request.url().to_string(), hash = calculate_hash(request), "Checking for recorded response");
        let map = self.requests.read().unwrap();
        let res = map.get(request);
        res.cloned()
    }

    fn partial_filepath(&self, request: &InMemoryRequest) -> PathBuf {
        let mut path = self.base_path.clone();
        path.push(request.host());
        path.push(&request.path()[1..]);
        path.push(request.method().as_str().to_lowercase());
        path
    }

    pub fn clear(&mut self) {
        self.requests.write().unwrap().clear();
    }

    pub fn record_response(&self, mut request: InMemoryRequest, mut response: InMemoryResponse) -> ProtocolResult<()> {
        let partial_path = self.partial_filepath(&request);
        sanitize_request(&mut request);
        sanitize_response(&mut response);

        let rr = RequestResponsePair { request, response };
        let stringified = serde_json::to_string_pretty(&rr).unwrap();
        let RequestResponsePair { request, response } = rr;
        let idx;
        {
            let mut write = self.requests.write().unwrap();
            let (i, _old) = write.insert_full(HashableRequest(request), response);
            idx = i;
        }
        let path = partial_path.with_extension(format!("{idx:04}.json"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, stringified)?;
        Ok(())
    }

    pub fn load_from_path(_path: &Path) {
        unimplemented!()
    }

    pub fn load_default() {
        unimplemented!()
    }
}

impl Default for RequestRecorder {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use http::Method;
    use http::Request;
    use std::hash::DefaultHasher;

    #[test]
    fn test_equal() {
        #[derive(Serialize, Deserialize, Debug)]
        struct Foobar {
            a: u32,
            b: u32,
        }
        let data = Foobar { a: 1, b: 2 };
        let original = Request::builder()
            .method(Method::POST)
            .uri("https://example.com/")
            .header("content-type", "application/json")
            .header("secret", "will-get-sanitized")
            .body(InMemoryBody::Json(serde_json::to_value(&data).unwrap()))
            .unwrap();
        let mut sanitized = HashableRequest(original.clone());
        let original = HashableRequest(original);
        sanitize_request(&mut sanitized.0);
        assert_eq!(
            original, sanitized,
            "The recorder stores sanitized requests, so these must be equal so that the sanitized request is returned on lookup."
        );
        assert_eq!(original.header("secret").unwrap(), "will-get-sanitized");
        assert_eq!(sanitized.header("secret").unwrap(), "**********");
        let h1 = {
            let mut s = DefaultHasher::new();
            original.hash(&mut s);
            s.finish()
        };
        let h2 = {
            let mut s = DefaultHasher::new();
            sanitized.hash(&mut s);
            s.finish()
        };
        assert_eq!(h1, h2);
    }
}
