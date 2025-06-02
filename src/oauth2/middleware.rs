use std::fmt::{Debug, Formatter};
use async_trait::async_trait;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::{header, HeaderName, InMemoryRequest, Method, Middleware, Next, RequestBuilder, ProtocolResult, Response, Client};
use super::refresh::RefreshResponse;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenType {
    Bearer,
    #[serde(untagged)]
    Other(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshData {
    pub access_token: String,
    pub token_type: TokenType,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
}

pub struct OAuth2 {
    // Configuration
    pub refresh_endpoint: String,
    pub client_id: String,
    pub client_secret: String,
    pub token_type: TokenType,

    // State
    pub access_token: RwLock<String>,
    pub refresh_token: String,

    pub callback: Option<Box<dyn Fn(RefreshData) + Send + Sync + 'static>>,
}

impl Debug for OAuth2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuth2")
            .field("refresh_endpoint", &self.refresh_endpoint)
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret)
            .field("token_type", &self.token_type)
            .field("access_token", &self.access_token)
            .field("refresh_token", &self.refresh_token)
            .field("callback", &self.callback.as_ref().map(|_| "Fn(RefreshData)"))
            .finish()
    }
}

impl OAuth2 {
    pub fn callback(&mut self, data: impl Fn(RefreshData) + Send + Sync + 'static) {
        self.callback = Some(Box::new(data));
    }

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

    pub fn make_refresh_request<'a>(&'a self, client: &'a Client) -> RequestBuilder<'a> {
        RequestBuilder::new(client, Method::POST, self.refresh_endpoint.parse().unwrap())
            .form(RefreshRequest {
                client_id: &self.client_id,
                client_secret: &self.client_secret,
                grant_type: "refresh_token",
                refresh_token: &self.refresh_token,
            })
    }
}

#[async_trait]
impl Middleware for OAuth2 {
    async fn handle(&self, request: InMemoryRequest, next: Next<'_>) -> ProtocolResult<Response> {
        let req = self.authorize(request);
        let res = next.run(req.clone().into()).await;
        if !matches!(&res, Ok(resp) if resp.status().as_u16() == 401) {
            // if we didn't get a 401, proceed as normal
            return res;
        }
        let refresh_req = self.make_refresh_request(next.client);
        let res = next.run(refresh_req.build()).await?;
        if res.status().is_client_error() || res.status().is_server_error() {
            return Ok(res);
        }
        let (_, body) = res.into_parts();
        let body = body.into_memory().await?;

        let data: RefreshResponse = body.json()?;
        {
            let mut access_token = self.access_token.write().unwrap();
            *access_token = data.access_token.clone();
        }
        if let Some(callback) = self.callback.as_ref() {
            callback(RefreshData {
                access_token: data.access_token,
                token_type: data.token_type,
                expires_in: data.expires_in,
                refresh_token: data.refresh_token,
            });
        }
        // // reauthorize the request with the newly set access token. it will overwrite the previously set headers
        let req = self.authorize(req);
        next.run(req.clone().into()).await
    }
}

#[derive(Debug, Serialize)]
struct RefreshRequest<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub grant_type: &'static str,
    pub refresh_token: &'a str,
}

