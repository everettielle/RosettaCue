use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::http;
use crate::{CompletionRequest, CompletionResponse, LlmError, LlmModel, ProviderConfig};

/// Selects the request shape for an OpenAI-compatible endpoint.
///
/// The two dialects diverge because GPT-5 reasoning models reject the sampling
/// parameters local servers rely on for determinism, and take the output cap
/// as `max_completion_tokens` rather than `max_tokens`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dialect {
    /// `api.openai.com`.
    OpenAi,
    /// LM Studio, Ollama, and other self-hosted OpenAI-compatible servers.
    LocalCompatible,
}

pub(super) fn complete(
    client: &Client,
    config: &ProviderConfig,
    request: &CompletionRequest<'_>,
    dialect: Dialect,
) -> Result<CompletionResponse, LlmError> {
    let payload = build_payload(config, request, dialect);
    let endpoint = http::endpoint(&config.base_url, "chat/completions");
    http::emit_request(config, "completion", "POST", &endpoint, Some(&payload));
    let response = http::authenticate_bearer(
        client.post(endpoint).json(&payload),
        config.api_key.as_deref(),
    )
    .send()
    .map_err(|error| {
        http::emit_transport_error(config, "completion", &error);
        http::map_request_error(error)
    })?;
    http::parse_response(response, "completion", parse_completion_response)
}

pub(super) fn list_models(
    client: &Client,
    config: &ProviderConfig,
) -> Result<Vec<LlmModel>, LlmError> {
    let endpoint = http::endpoint(&config.base_url, "models");
    http::emit_request(config, "list_models", "GET", &endpoint, None);
    let response = http::authenticate_bearer(client.get(endpoint), config.api_key.as_deref())
        .send()
        .map_err(|error| {
            http::emit_transport_error(config, "list_models", &error);
            http::map_request_error(error)
        })?;
    http::parse_response(response, "list_models", http::parse_model_list)
}

fn build_payload(
    config: &ProviderConfig,
    request: &CompletionRequest<'_>,
    dialect: Dialect,
) -> Value {
    let mut user_content = vec![json!({ "type": "text", "text": request.user_text })];
    if let Some(image) = request.image_png_base64 {
        user_content.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:image/png;base64,{image}") }
        }));
    }
    // OpenAI caches automatically on the longest identical message prefix, so the
    // stable block is folded into the system turn and the user turn is left with
    // per-request content only. Local servers ignore the distinction.
    let system = request.stable_context.map_or_else(
        || request.system.to_owned(),
        |stable| format!("{}\n\n{stable}", request.system),
    );
    let mut messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user_content }),
    ];
    if let (Some(previous), Some(correction)) = (request.previous_response, request.correction) {
        messages.push(json!({ "role": "assistant", "content": previous }));
        messages.push(json!({ "role": "user", "content": correction }));
    }
    let mut payload = json!({
        "model": config.model,
        "messages": messages,
        "stream": false
    });
    match dialect {
        Dialect::OpenAi => {
            payload["max_completion_tokens"] = json!(config.max_tokens);
            if let Some(effort) = config.reasoning_effort {
                payload["reasoning_effort"] = json!(effort.as_str());
            }
        }
        Dialect::LocalCompatible => {
            payload["max_tokens"] = json!(config.max_tokens);
            payload["temperature"] = json!(0);
            payload["seed"] = json!(0);
        }
    }
    if let Some(response_format) = request.response_format {
        payload["response_format"] = response_format.clone();
    }
    payload
}

fn parse_completion_response(body: &str) -> Result<CompletionResponse, LlmError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, ReasoningEffort};

    fn config(provider: LlmProvider) -> ProviderConfig {
        ProviderConfig {
            model: "vision-model".to_owned(),
            api_key: Some("test-key".to_owned()),
            timeout_seconds: 5,
            max_tokens: 100,
            max_attempts: 1,
            ..ProviderConfig::for_provider(provider)
        }
    }

    fn vision_request(schema: &Value) -> CompletionRequest<'_> {
        CompletionRequest {
            system: "system",
            stable_context: None,
            user_text: "read",
            image_png_base64: Some("cG5n"),
            response_format: Some(schema),
            previous_response: None,
            correction: None,
        }
    }

    #[test]
    fn local_dialect_preserves_sampling_parameters() {
        let schema = json!({ "type": "json_object" });
        let payload = build_payload(
            &config(LlmProvider::LmStudio),
            &vision_request(&schema),
            Dialect::LocalCompatible,
        );

        assert_eq!(payload["model"], "vision-model");
        assert_eq!(payload["temperature"], 0);
        assert_eq!(payload["seed"], 0);
        assert_eq!(payload["max_tokens"], 100);
        assert_eq!(payload["stream"], false);
        assert_eq!(payload["response_format"], schema);
        assert!(payload.get("max_completion_tokens").is_none());
        assert!(payload.get("reasoning_effort").is_none());
        assert_eq!(
            payload.pointer("/messages/1/content/1/image_url/url"),
            Some(&json!("data:image/png;base64,cG5n"))
        );
    }

    #[test]
    fn openai_dialect_drops_sampling_parameters_and_caps_completion_tokens() {
        let schema = json!({ "type": "json_object" });
        let payload = build_payload(
            &config(LlmProvider::OpenAi),
            &vision_request(&schema),
            Dialect::OpenAi,
        );

        assert!(
            payload.get("temperature").is_none(),
            "reasoning models reject temperature"
        );
        assert!(payload.get("seed").is_none());
        assert!(
            payload.get("max_tokens").is_none(),
            "reasoning models take max_completion_tokens"
        );
        assert_eq!(payload["max_completion_tokens"], 100);
        assert_eq!(payload["reasoning_effort"], "none");
        assert_eq!(payload["response_format"], schema);
    }

    #[test]
    fn openai_dialect_forwards_a_configured_reasoning_effort() {
        let mut config = config(LlmProvider::OpenAi);
        config.reasoning_effort = Some(ReasoningEffort::Low);
        let payload = build_payload(
            &config,
            &CompletionRequest {
                system: "system",
                stable_context: None,
                user_text: "read",
                image_png_base64: None,
                response_format: None,
                previous_response: None,
                correction: None,
            },
            Dialect::OpenAi,
        );

        assert_eq!(payload["reasoning_effort"], "low");
    }

    #[test]
    fn openai_dialect_omits_reasoning_effort_when_unset() {
        let mut config = config(LlmProvider::OpenAi);
        config.reasoning_effort = None;
        let payload = build_payload(
            &config,
            &CompletionRequest {
                system: "system",
                stable_context: None,
                user_text: "read",
                image_png_base64: None,
                response_format: None,
                previous_response: None,
                correction: None,
            },
            Dialect::OpenAi,
        );

        assert!(payload.get("reasoning_effort").is_none());
    }

    #[test]
    fn a_stable_block_is_folded_into_the_system_turn() {
        let payload = build_payload(
            &config(LlmProvider::OpenAi),
            &CompletionRequest {
                system: "system",
                stable_context: Some("stage guidance"),
                user_text: "row hint",
                image_png_base64: None,
                response_format: None,
                previous_response: None,
                correction: None,
            },
            Dialect::OpenAi,
        );

        assert_eq!(
            payload.pointer("/messages/0/content"),
            Some(&json!("system\n\nstage guidance"))
        );
        assert_eq!(
            payload.pointer("/messages/1/content/0/text"),
            Some(&json!("row hint")),
            "the user turn must carry per-request content only"
        );
    }

    #[test]
    fn retry_turns_follow_the_first_user_message() {
        let payload = build_payload(
            &config(LlmProvider::OpenAi),
            &CompletionRequest {
                system: "system",
                stable_context: None,
                user_text: "read",
                image_png_base64: None,
                response_format: None,
                previous_response: Some("{\"bad\":true}"),
                correction: Some("fix it"),
            },
            Dialect::OpenAi,
        );

        assert_eq!(payload.pointer("/messages/0/role"), Some(&json!("system")));
        assert_eq!(payload.pointer("/messages/1/role"), Some(&json!("user")));
        assert_eq!(
            payload.pointer("/messages/2/role"),
            Some(&json!("assistant"))
        );
        assert_eq!(
            payload.pointer("/messages/3/content"),
            Some(&json!("fix it"))
        );
    }

    #[test]
    fn completion_response_is_parsed_from_the_first_choice() {
        let parsed = parse_completion_response(
            r#"{"model":"vision-model","choices":[{"message":{"content":"{\"ok\":true}"}}],"usage":{"total_tokens":7}}"#,
        )
        .expect("OpenAI-compatible response");
        assert_eq!(parsed.content, "{\"ok\":true}");
        assert_eq!(parsed.model.as_deref(), Some("vision-model"));
    }
}
