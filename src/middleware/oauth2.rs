use std::str::FromStr;
use std::sync::{Mutex, RwLock};

use async_trait::async_trait;
use http::{header, HeaderName, method::Method, Uri};
use serde::{Deserialize, Serialize};

use crate::{Error, InMemoryRequest, Middleware, Request, RequestBuilder, Response, ResponseExt};
use crate::middleware::Next;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TokenType {
    Bearer,
    Other(String),
}

/// Response when requesting a new access token using a refresh token.
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub scope: String,
    pub token_type: TokenType,
}

/// Response when exchanging a code for an access token.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExchangeResponse {
    #[serde(flatten)]
    pub inner: RefreshResponse,
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshData {
    pub access_token: String,
}

#[derive(Debug)]
pub struct RefreshConfig {
    pub refresh_url: String,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshRequest<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub grant_type: &'static str,
    pub refresh_token: &'a str,
}

#[derive(Debug)]
pub struct Oauth2 {
    pub refresh_config: RefreshConfig,

    pub access_token: RwLock<String>,
    pub refresh_token: String,

    pub token_type: TokenType,

    pub callback: Option<fn(RefreshData) -> ()>
}

impl Oauth2 {
    fn authorize(&self, mut request: InMemoryRequest) -> InMemoryRequest {
        let access_token = self.access_token.read().unwrap();
        let access_token = access_token.as_str();
        match &self.token_type {
            TokenType::Bearer => {
                request.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {}", access_token).parse().unwrap());
            }
            TokenType::Other(s) => {
                request.headers_mut().insert(s.parse::<HeaderName>().unwrap(), access_token.parse().unwrap());
            }
        }
        request
    }
}

#[async_trait]
impl Middleware for Oauth2 {
    async fn handle(&self, request: Request, next: Next<'_>) -> Result<Response, Error> {
        let req = request.into_memory().await?;
        let req = self.authorize(req);
        let mut res = next.run(req.clone().into()).await?;
        let status = res.status().as_u16();
        if ![400, 401].contains(&status) {
            return Ok(res);
        }
        let refresh_req = RequestBuilder::new(next.client, Method::POST, Uri::from_str(&self.refresh_config.refresh_url).unwrap())
            .json(RefreshRequest {
                client_id: &self.refresh_config.client_id,
                client_secret: &self.refresh_config.client_secret,
                grant_type: "refresh_token",
                refresh_token: &self.refresh_token,
            })
            .build();
        let mut res = next.run(refresh_req.into()).await?;
        let data: RefreshResponse = res.json().await?;
        {
            let mut access_token = self.access_token.write().unwrap();
            *access_token = data.access_token.clone();
        }
        if let Some(callback) = self.callback {
            callback(RefreshData {
                access_token: data.access_token,
            });
        }
        // reauthorize the request with the newly set access token. it will overwrite the previously set headers
        let req = self.authorize(req);
        next.run(req.clone().into()).await
    }
}