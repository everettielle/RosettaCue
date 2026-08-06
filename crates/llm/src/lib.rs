use std::time::{Duration, Instant};

use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
        match self.config.provider {
            LlmProvider::Anthropic => self.complete_anthropic(request),
            LlmProvider::LmStudio | LlmProvider::Ollama | LlmProvider::OpenAi => {
                self.complete_openai_compatible(request)
            }
        }
    }

    fn complete_openai_compatible(
        &self,
        request: &CompletionRequest<'_>,
    ) -> Result<CompletionResponse, LlmError> {
        let payload = build_openai_payload(&self.config, request);
        let endpoint = endpoint(&self.config.base_url, "chat/completions");
        let response = authenticate(self.client.post(endpoint).json(&payload), &self.config)
            .send()
            .map_err(map_request_error)?;
        parse_response(response, parse_openai_response)
    }

    fn complete_anthropic(
        &self,
        request: &CompletionRequest<'_>,
    ) -> Result<CompletionResponse, LlmError> {
        let payload = build_anthropic_payload(&self.config, request);
        let endpoint = endpoint(&self.config.base_url, "messages");
        let response = authenticate(self.client.post(endpoint).json(&payload), &self.config)
            .send()
            .map_err(map_request_error)?;
        parse_response(response, parse_anthropic_response)
    }
}

fn build_openai_payload(config: &ProviderConfig, request: &CompletionRequest<'_>) -> Value {
    let mut user_content = vec![json!({ "type": "text", "text": request.user_text })];
    if let Some(image) = request.image_png_base64 {
        user_content.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:image/png;base64,{image}") }
        }));
    }
    let mut messages = vec![
        json!({ "role": "system", "content": request.system }),
        json!({ "role": "user", "content": user_content }),
    ];
    if let (Some(previous), Some(correction)) = (request.previous_response, request.correction) {
        messages.push(json!({ "role": "assistant", "content": previous }));
        messages.push(json!({ "role": "user", "content": correction }));
    }
    let mut payload = json!({
        "model": config.model,
        "messages": messages,
        "temperature": 0,
        "seed": 0,
        "max_tokens": config.max_tokens,
        "stream": false
    });
    if let Some(response_format) = request.response_format {
        payload["response_format"] = response_format.clone();
    }
    payload
}

fn build_anthropic_payload(config: &ProviderConfig, request: &CompletionRequest<'_>) -> Value {
    let mut instruction = request.user_text.to_owned();
    if let Some(schema) = request.response_format {
        instruction.push_str("\nReturn only JSON matching this response format:\n");
        instruction.push_str(&schema.to_string());
    }
    let mut user_content = vec![json!({ "type": "text", "text": instruction })];
    if let Some(image) = request.image_png_base64 {
        user_content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": image
            }
        }));
    }
    let mut messages = vec![json!({ "role": "user", "content": user_content })];
    if let (Some(previous), Some(correction)) = (request.previous_response, request.correction) {
        messages.push(json!({ "role": "assistant", "content": previous }));
        messages.push(json!({ "role": "user", "content": correction }));
    }
    let payload = json!({
        "model": config.model,
        "system": request.system,
        "messages": messages,
        "temperature": 0,
        "max_tokens": config.max_tokens,
        "stream": false
    });
    payload
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
    let response = authenticate(client.get(endpoint(base_url, "models")), &config)
        .send()
        .map_err(map_request_error)?;
    parse_response(response, |body| {
        let payload = serde_json::from_str::<Value>(body)?;
        let data = payload
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| LlmError::InvalidResponse("model list data is missing".to_owned()))?;
        let mut models = data
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .map(|id| LlmModel { id: id.to_owned() })
            .collect::<Vec<_>>();
        models.sort_by(|left, right| left.id.cmp(&right.id));
        models.dedup();
        Ok(models)
    })
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

fn authenticate(request: RequestBuilder, config: &ProviderConfig) -> RequestBuilder {
    match config.provider {
        LlmProvider::Anthropic => {
            let request = request.header("anthropic-version", "2023-06-01");
            if let Some(key) = config.api_key.as_deref().filter(|key| !key.is_empty()) {
                request.header("x-api-key", key)
            } else {
                request
            }
        }
        LlmProvider::LmStudio | LlmProvider::Ollama | LlmProvider::OpenAi => {
            if let Some(key) = config.api_key.as_deref().filter(|key| !key.is_empty()) {
                request.bearer_auth(key)
            } else {
                request
            }
        }
    }
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path)
}

fn parse_response<T>(
    response: reqwest::blocking::Response,
    parser: impl FnOnce(&str) -> Result<T, LlmError>,
) -> Result<T, LlmError> {
    let status = response.status();
    let body = response.text().map_err(map_request_error)?;
    if !status.is_success() {
        return Err(LlmError::Http {
            status: status.as_u16(),
            message: body.chars().take(1_000).collect(),
        });
    }
    parser(&body)
}

fn parse_openai_response(body: &str) -> Result<CompletionResponse, LlmError> {
    let payload = serde_json::from_str::<Value>(body)?;
    let content = payload
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::InvalidResponse("message content is missing".to_owned()))?;
    Ok(CompletionResponse {
        content: content.to_owned(),
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        usage: payload.get("usage").cloned().unwrap_or(Value::Null),
    })
}

fn parse_anthropic_response(body: &str) -> Result<CompletionResponse, LlmError> {
    let payload = serde_json::from_str::<Value>(body)?;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks.iter().find_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| block.get("text").and_then(Value::as_str))
                    .flatten()
            })
        })
        .ok_or_else(|| LlmError::InvalidResponse("text content is missing".to_owned()))?;
    Ok(CompletionResponse {
        content: content.to_owned(),
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        usage: payload.get("usage").cloned().unwrap_or(Value::Null),
    })
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

fn map_request_error(error: reqwest::Error) -> LlmError {
    if error.is_connect() || error.is_timeout() {
        LlmError::Unavailable(error.to_string())
    } else {
        LlmError::Request(error)
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_compatible_vision_contract() {
        let config = ProviderConfig {
            provider: LlmProvider::OpenAi,
            base_url: "https://api.openai.test/v1".to_owned(),
            model: "vision-model".to_owned(),
            api_key: Some("test-key".to_owned()),
            timeout_seconds: 5,
            max_tokens: 100,
            max_attempts: 1,
        };
        let payload = build_openai_payload(
            &config,
            &CompletionRequest {
                system: "system",
                user_text: "read",
                image_png_base64: Some("cG5n"),
                response_format: Some(&json!({ "type": "json_object" })),
                previous_response: None,
                correction: None,
            },
        );
        assert_eq!(payload["model"], "vision-model");
        assert_eq!(payload["stream"], false);
        assert_eq!(
            payload.pointer("/messages/1/content/1/image_url/url"),
            Some(&json!("data:image/png;base64,cG5n"))
        );
        let parsed = parse_openai_response(
            r#"{"model":"vision-model","choices":[{"message":{"content":"{\"ok\":true}"}}],"usage":{"total_tokens":7}}"#,
        )
        .expect("OpenAI response");
        assert_eq!(parsed.content, "{\"ok\":true}");
    }

    #[test]
    fn anthropic_messages_contract() {
        let config = ProviderConfig {
            provider: LlmProvider::Anthropic,
            base_url: "https://api.anthropic.test/v1".to_owned(),
            model: "claude-test".to_owned(),
            api_key: Some("anthropic-key".to_owned()),
            timeout_seconds: 5,
            max_tokens: 100,
            max_attempts: 1,
        };
        let payload = build_anthropic_payload(
            &config,
            &CompletionRequest {
                system: "system",
                user_text: "read",
                image_png_base64: Some("cG5n"),
                response_format: Some(&json!({ "type": "json_object" })),
                previous_response: None,
                correction: None,
            },
        );
        assert_eq!(payload["system"], "system");
        assert_eq!(
            payload.pointer("/messages/0/content/1/source/data"),
            Some(&json!("cG5n"))
        );
        let parsed = parse_anthropic_response(
            r#"{"model":"claude-test","content":[{"type":"text","text":"{\"ok\":true}"}],"usage":{"input_tokens":4}}"#,
        )
        .expect("Anthropic response");
        assert_eq!(parsed.model.as_deref(), Some("claude-test"));
    }

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
