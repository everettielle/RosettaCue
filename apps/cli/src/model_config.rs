//! The model configuration document accepted by `--config`.
//!
//! One JSON document describes every model profile a command needs, in the
//! same shape the rest of the application speaks: common fields flat,
//! provider-specific parameters nested under `provider_options`. Fields left
//! out resolve to the provider's defaults. The one deliberate difference from
//! the application's profile schema is credentials: a document lives on disk,
//! so it carries `api_key_env` — the name of an environment variable — and a
//! literal `api_key` is rejected outright.

use rosettacue_core::{
    LlmProvider, OcrPipelineConfig, ProviderConfig, ProviderSpec, ReasoningEffort,
};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfigDoc {
    recognition: ProfileDoc,
    /// Present selects separate ruby recognition; absent selects combined.
    #[serde(default)]
    ruby: Option<ProfileDoc>,
    /// Absent inherits the recognition profile.
    #[serde(default)]
    validation: Option<ProfileDoc>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDoc {
    provider: LlmProvider,
    #[serde(default)]
    provider_options: Option<ProviderOptionsDoc>,
    #[serde(default)]
    base_url: Option<String>,
    model: String,
    #[serde(default)]
    api_key_env: Option<String>,
    /// Captured only to be rejected with a pointer at `api_key_env`; without
    /// this field, `deny_unknown_fields` would reject it with a generic
    /// unknown-field error.
    #[serde(default)]
    api_key: Option<serde_json::Value>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    max_attempts: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderOptionsDoc {
    #[serde(default)]
    reasoning_effort: Option<ReasoningEffort>,
}

/// Loads an OCR pipeline configuration from a document path or inline JSON.
///
/// # Errors
///
/// Returns an error when the document cannot be read or parsed, or when a
/// profile fails to resolve (see [`resolve_profile`]).
pub fn load_pipeline(argument: &str) -> anyhow::Result<OcrPipelineConfig> {
    let doc: ModelConfigDoc = parse(argument)?;
    let recognition = resolve_profile(doc.recognition, "recognition")?;
    let ruby = doc
        .ruby
        .map(|profile| resolve_profile(profile, "ruby"))
        .transpose()?;
    let validation = doc
        .validation
        .map(|profile| resolve_profile(profile, "validation"))
        .transpose()?
        .unwrap_or_else(|| recognition.clone());
    Ok(OcrPipelineConfig {
        recognition,
        ruby,
        validation,
    })
}

/// Loads a single profile from a document path or inline JSON.
///
/// # Errors
///
/// Returns an error when the document cannot be read or parsed, or when the
/// profile fails to resolve (see [`resolve_profile`]).
pub fn load_profile(argument: &str) -> anyhow::Result<ProviderConfig> {
    resolve_profile(parse(argument)?, "profile")
}

fn parse<T: serde::de::DeserializeOwned>(argument: &str) -> anyhow::Result<T> {
    let text = if argument.trim_start().starts_with('{') {
        argument.to_owned()
    } else {
        std::fs::read_to_string(argument).map_err(|error| {
            anyhow::anyhow!("could not read config document {argument}: {error}")
        })?
    };
    serde_json::from_str(&text)
        .map_err(|error| anyhow::anyhow!("invalid model config document: {error}"))
}

/// Fills a partial profile document with provider defaults and resolves the
/// API key environment variable.
///
/// # Errors
///
/// Returns an error for a literal `api_key`, a `provider_options` block on a
/// provider that takes none, or an unreadable key variable.
fn resolve_profile(doc: ProfileDoc, label: &str) -> anyhow::Result<ProviderConfig> {
    anyhow::ensure!(
        doc.api_key.is_none(),
        "{label}: api_key must not be stored in a config document; \
         set api_key_env to the name of an environment variable holding the key"
    );
    let kind = doc.provider;
    let provider = match kind {
        LlmProvider::OpenAi => ProviderSpec::OpenAi {
            reasoning_effort: doc
                .provider_options
                .and_then(|options| options.reasoning_effort)
                .unwrap_or(ReasoningEffort::None),
        },
        LlmProvider::LmStudio | LlmProvider::Ollama | LlmProvider::Anthropic => {
            anyhow::ensure!(
                doc.provider_options.is_none(),
                "{label}: provider_options is not accepted for {}",
                kind.as_str()
            );
            ProviderSpec::default_for(kind)
        }
    };
    let defaults = ProviderConfig::for_provider(kind);
    Ok(ProviderConfig {
        provider,
        base_url: doc.base_url.unwrap_or(defaults.base_url),
        model: doc.model,
        api_key: crate::read_api_key(doc.api_key_env.as_deref())?,
        timeout_seconds: doc.timeout_seconds.unwrap_or(defaults.timeout_seconds),
        max_tokens: doc.max_tokens.unwrap_or(defaults.max_tokens),
        max_attempts: doc.max_attempts.unwrap_or(defaults.max_attempts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_profile_fills_provider_defaults() {
        let config = load_profile(r#"{ "provider": "lm_studio", "model": "gemma-test" }"#)
            .expect("minimal profile");
        assert_eq!(config.provider, ProviderSpec::LmStudio);
        assert_eq!(config.base_url, "http://127.0.0.1:1234/v1");
        assert_eq!(config.model, "gemma-test");
        assert_eq!(config.timeout_seconds, 120);
        assert_eq!(config.max_tokens, 512);
        assert_eq!(config.max_attempts, 2);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn provider_options_resolve_into_the_openai_spec() {
        let config = load_profile(
            r#"{
                "provider": "open_ai",
                "model": "gpt-5.6-luna",
                "provider_options": { "reasoning_effort": "low" }
            }"#,
        )
        .expect("openai profile");
        assert_eq!(
            config.provider,
            ProviderSpec::OpenAi {
                reasoning_effort: ReasoningEffort::Low
            }
        );
    }

    #[test]
    fn an_openai_profile_without_options_defaults_to_no_reasoning() {
        let config = load_profile(r#"{ "provider": "open_ai", "model": "gpt-5.6-luna" }"#)
            .expect("openai profile");
        assert_eq!(
            config.provider,
            ProviderSpec::OpenAi {
                reasoning_effort: ReasoningEffort::None
            }
        );
    }

    #[test]
    fn provider_options_on_a_provider_that_takes_none_are_rejected() {
        let error = load_profile(
            r#"{
                "provider": "lm_studio",
                "model": "gemma-test",
                "provider_options": { "reasoning_effort": "low" }
            }"#,
        )
        .expect_err("options on lm_studio");
        assert!(error.to_string().contains("provider_options"));
    }

    #[test]
    fn a_literal_api_key_is_rejected_with_a_pointer_at_the_env_indirection() {
        let error = load_profile(
            r#"{ "provider": "open_ai", "model": "gpt-5.6-luna", "api_key": "sk-secret" }"#,
        )
        .expect_err("literal key");
        assert!(error.to_string().contains("api_key_env"));
    }

    #[test]
    fn an_unknown_field_is_rejected_rather_than_ignored() {
        let error = load_profile(r#"{ "provider": "lm_studio", "model": "m", "baseurl": "x" }"#)
            .expect_err("typo field");
        assert!(error.to_string().contains("baseurl"));
    }

    #[test]
    fn the_api_key_environment_variable_is_resolved_at_load() {
        // PATH is the one variable every test environment defines; setting a
        // dedicated variable would need unsafe env mutation.
        let config = load_profile(
            r#"{
                "provider": "anthropic",
                "model": "claude-test",
                "api_key_env": "PATH"
            }"#,
        )
        .expect("profile with key env");
        assert_eq!(config.api_key, std::env::var("PATH").ok());
    }

    #[test]
    fn an_unreadable_api_key_environment_variable_is_an_error() {
        let error = load_profile(
            r#"{
                "provider": "anthropic",
                "model": "claude-test",
                "api_key_env": "ROSETTACUE_TEST_UNSET_KEY"
            }"#,
        )
        .expect_err("unset variable");
        assert!(error.to_string().contains("ROSETTACUE_TEST_UNSET_KEY"));
    }

    #[test]
    fn an_omitted_validation_profile_inherits_recognition() {
        let pipeline = load_pipeline(
            r#"{
                "recognition": { "provider": "lm_studio", "model": "gemma-test" }
            }"#,
        )
        .expect("pipeline");
        assert!(
            pipeline.ruby.is_none(),
            "omitted ruby selects combined mode"
        );
        assert_eq!(pipeline.validation.model, pipeline.recognition.model);
        assert_eq!(pipeline.validation.base_url, pipeline.recognition.base_url);
    }

    #[test]
    fn a_present_ruby_profile_selects_separate_recognition() {
        let pipeline = load_pipeline(
            r#"{
                "recognition": { "provider": "lm_studio", "model": "gemma-test" },
                "ruby": { "provider": "lm_studio", "model": "ruby-specialist" }
            }"#,
        )
        .expect("pipeline");
        assert_eq!(
            pipeline.ruby.expect("ruby profile").model,
            "ruby-specialist"
        );
    }

    #[test]
    fn a_missing_document_file_reports_the_path() {
        let error = load_profile("/nonexistent/config.json").expect_err("missing file");
        assert!(error.to_string().contains("/nonexistent/config.json"));
    }
}
