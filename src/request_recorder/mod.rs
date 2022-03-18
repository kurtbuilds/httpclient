
use std::collections::{HashMap};
use std::fs;


use std::path::{Path, PathBuf};

use walkdir::WalkDir;
use crate::{Request, Response};

use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseWithRequest {
    pub request: Request,
    pub response: Response,
}


pub struct RequestRecorder {
    pub base_path: PathBuf,
    pub requests: HashMap<Request, Response>
}


pub fn load_requests(path: &PathBuf) -> impl Iterator<Item=ResponseWithRequest> {
    WalkDir::new(&path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name().to_str().unwrap().ends_with(".json"))
        .flat_map(|filepath| {
            let f = fs::File::open(filepath.path()).unwrap();
            let res = serde_json::from_reader::<_, Vec<ResponseWithRequest>>(&f).unwrap_or_default();
            res.into_iter()
        })
}

impl RequestRecorder {
    pub fn new() -> Self {
        let path = std::env::current_dir().unwrap().join("data").join("vcr");
        println!("Request recorder opened at {}", path.display());
        let requests = load_requests(&path)
            .map(|r| (r.request, r.response))
            .collect();
        RequestRecorder {
            base_path: path,
            requests,
        }
    }

    pub fn recorded_response(&self, request: &Request) -> Option<Response> {
        self.requests.get(request).map(|r| r.try_clone().unwrap())
    }

    fn recording_path(&self, request: &Request) -> PathBuf {
        let mut path = self.base_path.clone();
        path.push(request.host());
        path.push(&request.path()[1..]);
        path.push(request.method().as_str().to_lowercase() + ".json");
        path
    }

    pub async fn record_response(&self, request: Request, response: Response) -> Result<Response, crate::Error> {
        let path = self.recording_path(&request);
        println!("Recording response to {}", path.display());
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut map = if let Ok(f) =fs::File::open(&path) {
            let res = serde_json::from_reader::<_, Vec<ResponseWithRequest>>(&f).unwrap_or_default();
            let map: HashMap<Request, Response> = HashMap::from_iter(res.into_iter().map(|r| (r.request, r.response)));
            map
        } else {
            HashMap::new()
        };
        let response = response.into_infallible_cloneable().await?;
        println!("Recording response: {:?}", response);
        map.insert(request.try_clone().unwrap(), response.try_clone().unwrap());
        let f = fs::File::create(&path)?;
        let res = map.into_iter().map(|(k, v)| ResponseWithRequest { request: k, response: v }).collect::<Vec<_>>();
        serde_json::to_writer_pretty(f, &res).map_err(crate::Error::from)?;
        Ok(response)
    }

    pub fn load_from_path(_path: &Path) {
        unimplemented!()
    }

    pub fn load_default() {
        unimplemented!()
    }
}