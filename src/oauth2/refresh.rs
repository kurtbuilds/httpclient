use serde::{Deserialize, Serialize};

use super::middleware::TokenType;

#[derive(Debug)]
pub struct RefreshConfig {
}

/// Response when requesting a new access token using a refresh token.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RefreshResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub scope: Option<String>,
    pub token_type: TokenType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}