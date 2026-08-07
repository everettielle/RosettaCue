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

    /// Whether the provider is a hosted API that requires an API key.
    #[must_use]
    pub const fn is_remote(self) -> bool {
        matches!(self, Self::OpenAi | Self::Anthropic)
    }
}

/// Reasoning depth for `OpenAI` reasoning models.
///
/// Reasoning tokens are billed at the output rate. OCR and translation are
/// transcription tasks rather than deliberation, so profiles default to
/// [`ReasoningEffort::None`]; leaving the parameter unset would let the
/// server-side default (`medium`) multiply output cost with no accuracy gain.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    #[default]
    None,
    Minimal,
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Provider selection together with the parameters only that provider accepts.
///
/// A provider-specific parameter lives inside its provider's variant, so a
/// configuration carrying another provider's parameter is unrepresentable and
/// needs no cross-field validation. In profile JSON the selection stays flat
/// while the parameters nest under a `provider_options` block:
/// `{"provider": "open_ai", "provider_options": {"reasoning_effort": "none"}}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ProviderSpecWire", into = "ProviderSpecWire")]
pub enum ProviderSpec {
    LmStudio,
    Ollama,
    OpenAi { reasoning_effort: ReasoningEffort },
    Anthropic,
}

/// Wire shape for [`ProviderSpec`]: the provider tag plus an optional
/// `provider_options` block.
///
/// The conversion, rather than a derived tagged enum, defines how imperfect
/// stored data resolves: a missing or null block falls back to the provider's
/// defaults, and a stray block on a provider that takes no options is dropped
/// rather than rejected.
#[derive(Serialize, Deserialize)]
struct ProviderSpecWire {
    provider: LlmProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_options: Option<ProviderOptionsWire>,
}

/// Every provider-specific parameter as it appears inside `provider_options`.
///
/// Fields for all providers share this one wire struct; the conversion picks
/// the ones belonging to the tagged provider and ignores the rest.
#[derive(Default, Serialize, Deserialize)]
struct ProviderOptionsWire {
    #[serde(default)]
    reasoning_effort: ReasoningEffort,
}

impl From<ProviderSpecWire> for ProviderSpec {
    fn from(wire: ProviderSpecWire) -> Self {
        match wire.provider {
            LlmProvider::OpenAi => Self::OpenAi {
                reasoning_effort: wire.provider_options.unwrap_or_default().reasoning_effort,
            },
            kind => Self::default_for(kind),
        }
    }
}

impl From<ProviderSpec> for ProviderSpecWire {
    fn from(spec: ProviderSpec) -> Self {
        Self {
            provider: spec.kind(),
            provider_options: match spec {
                ProviderSpec::OpenAi { reasoning_effort } => {
                    Some(ProviderOptionsWire { reasoning_effort })
                }
                ProviderSpec::LmStudio | ProviderSpec::Ollama | ProviderSpec::Anthropic => None,
            },
        }
    }
}

impl ProviderSpec {
    #[must_use]
    pub const fn kind(self) -> LlmProvider {
        match self {
            Self::LmStudio => LlmProvider::LmStudio,
            Self::Ollama => LlmProvider::Ollama,
            Self::OpenAi { .. } => LlmProvider::OpenAi,
            Self::Anthropic => LlmProvider::Anthropic,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.kind().as_str()
    }

    /// Whether the provider is a hosted API that requires an API key.
    #[must_use]
    pub const fn is_remote(self) -> bool {
        self.kind().is_remote()
    }

    /// The default spec for a provider kind.
    #[must_use]
    pub const fn default_for(kind: LlmProvider) -> Self {
        match kind {
            LlmProvider::LmStudio => Self::LmStudio,
            LlmProvider::Ollama => Self::Ollama,
            LlmProvider::OpenAi => Self::OpenAi {
                reasoning_effort: ReasoningEffort::None,
            },
            LlmProvider::Anthropic => Self::Anthropic,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(flatten)]
    pub provider: ProviderSpec,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout_seconds: u64,
    pub max_tokens: u32,
    pub max_attempts: u32,
}

impl ProviderConfig {
    #[must_use]
    pub fn for_provider(provider: LlmProvider) -> Self {
        Self {
            provider: ProviderSpec::default_for(provider),
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
        if self.provider.is_remote() && self.api_key.as_deref().is_none_or(str::is_empty) {
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
    /// Instruction text that is byte-identical across every request of the same
    /// stage, language, and schema.
    ///
    /// Providers that support explicit prompt caching place a cache breakpoint at
    /// the end of this block, so callers must keep per-request values — row hints,
    /// recognized lines, the image — in `user_text`. A single varying byte here
    /// invalidates the cache for every later request.
    pub stable_context: Option<&'a str>,
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
        if let Some(api_key) = config.api_key.as_deref() {
            rosettacue_diagnostics::register_secret(api_key);
        }
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
    if provider.is_remote() && api_key.is_none_or(str::is_empty) {
        return Err(LlmError::InvalidConfig(format!(
            "{} requires an API key",
            provider.as_str()
        )));
    }
    if let Some(api_key) = api_key {
        rosettacue_diagnostics::register_secret(api_key);
    }
    let config = ProviderConfig {
        provider: ProviderSpec::default_for(provider),
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

    #[test]
    fn default_spec_carries_reasoning_effort_for_openai_only() {
        assert_eq!(
            ProviderConfig::for_provider(LlmProvider::OpenAi).provider,
            ProviderSpec::OpenAi {
                reasoning_effort: ReasoningEffort::None
            }
        );
        assert_eq!(
            ProviderConfig::for_provider(LlmProvider::Anthropic).provider,
            ProviderSpec::Anthropic
        );
    }

    #[test]
    fn provider_options_nest_under_their_own_block_in_profile_json() {
        let openai = serde_json::to_value(ProviderConfig::for_provider(LlmProvider::OpenAi))
            .expect("openai profile");
        assert_eq!(openai["provider"], "open_ai");
        assert_eq!(openai["provider_options"]["reasoning_effort"], "none");
        assert_eq!(openai["base_url"], "https://api.openai.com/v1");
        assert!(
            openai.get("reasoning_effort").is_none(),
            "provider-specific parameters must not leak into the common namespace"
        );

        let anthropic = serde_json::to_value(ProviderConfig::for_provider(LlmProvider::Anthropic))
            .expect("anthropic profile");
        assert_eq!(anthropic["provider"], "anthropic");
        assert!(anthropic.get("provider_options").is_none());
    }

    #[test]
    fn profiles_round_trip_through_json_for_every_provider() {
        for provider in [
            LlmProvider::LmStudio,
            LlmProvider::Ollama,
            LlmProvider::OpenAi,
            LlmProvider::Anthropic,
        ] {
            let config = ProviderConfig::for_provider(provider);
            let json = serde_json::to_value(&config).expect("serialize");
            let parsed = serde_json::from_value::<ProviderConfig>(json).expect("deserialize");
            assert_eq!(parsed.provider, config.provider);
        }
    }

    #[test]
    fn an_openai_profile_without_a_provider_options_block_defaults_to_none() {
        for provider_options in [None, Some(serde_json::Value::Null)] {
            let mut stored = serde_json::json!({
                "provider": "open_ai",
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.6-luna",
                "api_key": null,
                "timeout_seconds": 120,
                "max_tokens": 512,
                "max_attempts": 2
            });
            if let Some(null_block) = provider_options {
                stored["provider_options"] = null_block;
            }
            let config = serde_json::from_value::<ProviderConfig>(stored).expect("stored profile");
            assert_eq!(
                config.provider,
                ProviderSpec::OpenAi {
                    reasoning_effort: ReasoningEffort::None
                }
            );
        }
    }

    #[test]
    fn a_stray_provider_options_block_on_another_provider_is_dropped() {
        let stored = serde_json::json!({
            "provider": "anthropic",
            "base_url": "https://api.anthropic.com/v1",
            "model": "claude-test",
            "api_key": null,
            "timeout_seconds": 120,
            "max_tokens": 512,
            "max_attempts": 2,
            "provider_options": { "reasoning_effort": "low" }
        });
        let config = serde_json::from_value::<ProviderConfig>(stored).expect("stored profile");
        assert_eq!(config.provider, ProviderSpec::Anthropic);
    }
}
