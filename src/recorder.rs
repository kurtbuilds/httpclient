use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use walkdir::WalkDir;

use crate::{InMemoryRequest, InMemoryResponse, InMemoryResult};
use crate::response::{clone_inmemory_response, InMemoryResponseExt};

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestResponsePair {
    pub request: InMemoryRequest,
    #[serde(with = "crate::response::serde_response")]
    pub response: InMemoryResponse,
}

#[derive(Debug)]
pub struct RRPair {
    pub request: InMemoryRequest,
    pub response: InMemoryResponse,
    pub fname: String,
}


pub struct RequestRecorder {
    pub base_path: PathBuf,
    pub requests: Arc<RwLock<IndexMap<InMemoryRequest, InMemoryResponse>>>,
}


fn load_requests(path: &PathBuf) -> impl Iterator<Item=RRPair> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name().to_str().unwrap().ends_with(".json"))
        .map(|filepath| {
            debug!(file=filepath.path().display().to_string(), "Loading recording");
            let f = fs::read_to_string(filepath.path()).unwrap();
            let rr: RequestResponsePair = serde_json::from_str(&f).unwrap();
            RRPair {
                request: rr.request,
                response: rr.response,
                fname: filepath.path().file_name().unwrap().to_str().unwrap().to_string(),
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
        debug!(dir=path.display().to_string(), "Request recorder created");
        let mut requests = load_requests(&path).collect::<Vec<_>>();
        requests.sort_by_key(|rr| rr.fname.clone());
        let requests: IndexMap<InMemoryRequest, InMemoryResponse> = requests.into_iter()
            .map(|r| (r.request, r.response))
            .collect::<_>();
        info!(num_recordings=requests.len(), dir=path.display().to_string(), "Request recorder loaded");
        let requests = Arc::new(RwLock::new(requests));
        RequestRecorder {
            base_path: path,
            requests,
        }
    }

    pub fn get_response(&self, request: &InMemoryRequest) -> Option<InMemoryResponse> {
        debug!(url=request.url().to_string(), hash=calculate_hash(request), "Checking for recorded response");
        self.requests.read().unwrap().get(request).map(clone_inmemory_response)
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

    pub fn record_response(&self, mut request: InMemoryRequest, mut response: InMemoryResponse) -> InMemoryResult<()> {
        let partial_path = self.partial_filepath(&request);
        request.sanitize();
        response.sanitize();

        let rr = RequestResponsePair {
            request,
            response,
        };
        let stringified = serde_json::to_string_pretty(&rr).unwrap();
        let RequestResponsePair { request, response } = rr;
        let idx;
        {
            let mut write = self.requests.write().unwrap();
            let (i, _old) = write.insert_full(request, response);
            idx = i;
        }
        let path = partial_path.with_extension(format!("{:04}.json", idx));
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