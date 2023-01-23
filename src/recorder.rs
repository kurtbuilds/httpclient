use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{InMemoryRequest, InMemoryResponse, InMemoryResult};


#[derive(Serialize, Deserialize, Debug)]
pub struct RequestResponsePair {
    pub request: InMemoryRequest,
    pub response: InMemoryResponse,
}


pub struct RequestRecorder {
    pub base_path: PathBuf,
    pub requests: HashMap<InMemoryRequest, InMemoryResponse>,
}


fn load_requests(path: &PathBuf) -> impl Iterator<Item=RequestResponsePair> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name().to_str().unwrap().ends_with(".json"))
        .flat_map(|filepath| {
            let f = fs::File::open(filepath.path()).unwrap();
            let res = serde_json::from_reader::<_, Vec<RequestResponsePair>>(&f).unwrap();
            res.into_iter()
        })
}

impl RequestRecorder {
    pub fn new() -> Self {
        let path = std::env::current_dir().unwrap().join("data").join("vcr");
        println!("Request recorder opened at {}", path.display());
        // println!("Request recorder opened at {}", path.display());
        #[allow(clippy::mutable_key_type)]
            let requests: HashMap<InMemoryRequest, InMemoryResponse> = load_requests(&path)
            .map(|r| (r.request, r.response))
            .collect();
        println!("Loaded {} requests", requests.len());
        RequestRecorder {
            base_path: path,
            requests,
        }
    }

    pub fn get_response(&self, request: &InMemoryRequest) -> Option<InMemoryResponse> {
        println!("Looking for response for request: {:?}", request);
        println!("Requests: {:?}", self.requests.keys().collect::<Vec<_>>());
        self.requests.get(request).cloned()
    }

    fn filepath_for_request(&self, request: &InMemoryRequest) -> PathBuf {
        let mut path = self.base_path.clone();
        path.push(request.host());
        path.push(&request.path()[1..]);
        path.push(request.method().as_str().to_lowercase() + ".json");
        path
    }

    pub fn record_response(&self, mut request: InMemoryRequest, mut response: InMemoryResponse) -> InMemoryResult<()> {
        let path = self.filepath_for_request(&request);
        // println!("Recording response to {}", path.display());
        fs::create_dir_all(path.parent().unwrap())?;
        #[allow(clippy::mutable_key_type)]
            let mut map = if let Ok(f) = fs::File::open(&path) {
            let res = serde_json::from_reader::<_, Vec<RequestResponsePair>>(&f).unwrap_or_default();
            HashMap::from_iter(res.into_iter().map(|r| (r.request, r.response)))
        } else {
            HashMap::new()
        };
        // println!("Recording response: {:?}", response);
        request.sanitize();
        response.sanitize();
        map.insert(request, response);
        let f = fs::File::create(&path)?;
        let res = map.into_iter().map(|(k, v)| RequestResponsePair { request: k, response: v }).collect::<Vec<_>>();
        serde_json::to_writer_pretty(f, &res)?;
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