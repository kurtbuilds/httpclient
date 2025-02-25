use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptType {
    None,
    Consent,
    SelectAccount,
    #[serde(untagged)]
    Other(String)
}


/// Parameters for initializing the OAuth2 flow.
#[derive(Debug, Clone, Serialize)]
pub struct Initialize {
    pub scope: String,
    pub access_type: AccessType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub prompt: PromptType,
}

impl Initialize {
    pub fn no_state(scope: String, access_type: AccessType) -> Self {
        Self {
            scope,
            access_type,
            state: None,
            prompt: PromptType::None,
        }
    }

    pub fn prompt(mut self, prompt: PromptType) -> Self {
        self.prompt = prompt;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessType {
    Offline,
    #[serde(untagged)]
    Other(String),
}

#[derive(Debug, Serialize)]
pub(super) struct InitializeParams<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    /// value should be "code". TODO to remove the field from the struct
    pub response_type: &'static str,
    pub scope: String,
    pub access_type: AccessType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub prompt: PromptType,
}
