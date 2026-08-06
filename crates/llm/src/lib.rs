use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod http;
mod providers;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    LmStudio,
    Ollama,
    OpenAi,
    Anthropic,
}

impl LlmProvider {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LmStudio => "lmstudio",
            Self::Ollama => "ollama",
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }

    #[must_use]
    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::LmStudio => "http://127.0.0.1:1234/v1",
            Self::Ollama => "http://127.0.0.1:11434/v1",
            Self::OpenAi => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com/v1",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default = "default_provider")]
    pub provider: LlmProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout_seconds: u64,
    pub max_tokens: u32,
    pub max_attempts: u32,
}

const fn default_provider() -> LlmProvider {
    LlmProvider::LmStudio
}

impl ProviderConfig {
    #[must_use]
    pub fn for_provider(provider: LlmProvider) -> Self {
        Self {
            provider,
            base_url: provider.default_base_url().to_owned(),
            model: String::new(),
            api_key: None,
            timeout_seconds: 120,
            max_tokens: 512,
            max_attempts: 2,
        }
    }

    /// Validates fields required for an inference request.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid endpoint, missing model, missing remote-provider key,
    /// or non-positive execution limits.
    pub fn validate(&self) -> Result<(), LlmError> {
        validate_base_url(&self.base_url)?;
        if self.model.trim().is_empty() {
            return Err(LlmError::InvalidConfig("model is required".to_owned()));
        }
        if matches!(self.provider, LlmProvider::OpenAi | LlmProvider::Anthropic)
            && self.api_key.as_deref().is_none_or(str::is_empty)
        {
            return Err(LlmError::InvalidConfig(format!(
                "{} requires an API key",
                self.provider.as_str()
            )));
        }
        if self.timeout_seconds == 0 || self.max_tokens == 0 || self.max_attempts == 0 {
            return Err(LlmError::InvalidConfig(
                "timeout, max tokens and max attempts must be positive".to_owned(),
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn redacted(&self) -> Self {
        let mut redacted = self.clone();
        redacted.api_key = None;
        redacted
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self::for_provider(LlmProvider::LmStudio)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmModel {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDiagnostic {
    pub provider: LlmProvider,
    pub reachable: bool,
    pub latency_ms: u64,
    pub models: Vec<LlmModel>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest<'a> {
    pub system: &'a str,
    pub user_text: &'a str,
    pub image_png_base64: Option<&'a str>,
    pub response_format: Option<&'a Value>,
    pub previous_response: Option<&'a str>,
    pub correction: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub model: Option<String>,
    pub usage: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("invalid provider configuration: {0}")]
    InvalidConfig(String),
    #[error("could not reach provider: {0}")]
    Unavailable(String),
    #[error("provider returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

pub struct ProviderClient {
    config: ProviderConfig,
    client: Client,
}

impl ProviderClient {
    /// Creates a provider client for inference.
    ///
    /// # Errors
    ///
    /// Returns an error when the configuration or HTTP client is invalid.
    pub fn new(config: ProviderConfig) -> Result<Self, LlmError> {
        config.validate()?;
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()?;
        Ok(Self { config, client })
    }

    #[must_use]
    pub const fn config(&self) -> &ProviderConfig {
        &self.config
    }

    /// Runs one text or vision completion through the selected provider.
    ///
    /// # Errors
    ///
    /// Returns an error for transport, HTTP, or response-contract failures.
    pub fn complete(
        &self,
        request: &CompletionRequest<'_>,
    ) -> Result<CompletionResponse, LlmError> {
        providers::complete(&self.client, &self.config, request)
    }
}

/// Lists models exposed by a configured provider endpoint.
///
/// # Errors
///
/// Returns an error for invalid endpoints, transport failures, HTTP errors, or malformed data.
pub fn list_models(
    provider: LlmProvider,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<LlmModel>, LlmError> {
    validate_base_url(base_url)?;
    if matches!(provider, LlmProvider::OpenAi | LlmProvider::Anthropic)
        && api_key.is_none_or(str::is_empty)
    {
        return Err(LlmError::InvalidConfig(format!(
            "{} requires an API key",
            provider.as_str()
        )));
    }
    let config = ProviderConfig {
        provider,
        base_url: base_url.to_owned(),
        model: "diagnostic".to_owned(),
        api_key: api_key.map(str::to_owned),
        timeout_seconds: 10,
        max_tokens: 1,
        max_attempts: 1,
    };
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
    providers::list_models(&client, &config)
}

#[must_use]
pub fn diagnose(
    provider: LlmProvider,
    base_url: &str,
    api_key: Option<&str>,
) -> ProviderDiagnostic {
    let started = Instant::now();
    match list_models(provider, base_url, api_key) {
        Ok(models) => ProviderDiagnostic {
            provider,
            reachable: true,
            latency_ms: elapsed_ms(started),
            message: format!("connected; {} model(s) available", models.len()),
            models,
        },
        Err(error) => ProviderDiagnostic {
            provider,
            reachable: false,
            latency_ms: elapsed_ms(started),
            models: Vec::new(),
            message: error.to_string(),
        },
    }
}

fn validate_base_url(base_url: &str) -> Result<(), LlmError> {
    let url = reqwest::Url::parse(base_url)
        .map_err(|error| LlmError::InvalidConfig(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(LlmError::InvalidConfig(
            "base URL must be an HTTP(S) endpoint".to_owned(),
        ));
    }
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_providers_require_keys_but_local_providers_do_not() {
        let mut config = ProviderConfig::for_provider(LlmProvider::OpenAi);
        config.model = "gpt-test".to_owned();
        assert!(matches!(config.validate(), Err(LlmError::InvalidConfig(_))));

        let mut local = ProviderConfig::for_provider(LlmProvider::Ollama);
        local.model = "gemma-test".to_owned();
        assert!(local.validate().is_ok());
    }
}
