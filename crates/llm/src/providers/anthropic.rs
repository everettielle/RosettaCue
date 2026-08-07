use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{Value, json};

use crate::http;
use crate::{CompletionRequest, CompletionResponse, LlmError, LlmModel, ProviderConfig};

pub(super) fn complete(
    client: &Client,
    config: &ProviderConfig,
    request: &CompletionRequest<'_>,
) -> Result<CompletionResponse, LlmError> {
    let payload = build_payload(config, request);
    let endpoint = http::endpoint(&config.base_url, "messages");
    http::emit_request(config, "completion", "POST", &endpoint, Some(&payload));
    let response = authenticate(client.post(endpoint).json(&payload), config)
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
    let response = authenticate(client.get(endpoint), config)
        .send()
        .map_err(|error| {
            http::emit_transport_error(config, "list_models", &error);
            http::map_request_error(error)
        })?;
    http::parse_response(response, "list_models", http::parse_model_list)
}

fn authenticate(request: RequestBuilder, config: &ProviderConfig) -> RequestBuilder {
    let request = request.header("anthropic-version", "2023-06-01");
    if let Some(key) = config.api_key.as_deref().filter(|key| !key.is_empty()) {
        request.header("x-api-key", key)
    } else {
        request
    }
}

const SCHEMA_INSTRUCTION: &str = "\nReturn only JSON matching this response format:\n";

fn build_payload(config: &ProviderConfig, request: &CompletionRequest<'_>) -> Value {
    let schema_block = request
        .response_format
        .map(|schema| format!("{SCHEMA_INSTRUCTION}{schema}"));

    // With a stable block the schema rides at the end of the cached prefix and the
    // user turn carries only per-request content. Without one the schema stays
    // appended to the instruction, which is the pre-caching contract.
    let (system, instruction) = if let Some(stable) = request.stable_context {
        let mut cached = stable.to_owned();
        if let Some(schema) = &schema_block {
            cached.push_str(schema);
        }
        (
            json!([
                { "type": "text", "text": request.system },
                {
                    "type": "text",
                    "text": cached,
                    "cache_control": { "type": "ephemeral" }
                }
            ]),
            request.user_text.to_owned(),
        )
    } else {
        let mut instruction = request.user_text.to_owned();
        if let Some(schema) = &schema_block {
            instruction.push_str(schema);
        }
        (json!(request.system), instruction)
    };

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
    json!({
        "model": config.model,
        "system": system,
        "messages": messages,
        "max_tokens": config.max_tokens,
        "stream": false
    })
}

fn parse_completion_response(body: &str) -> Result<CompletionResponse, LlmError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LlmProvider;

    fn config() -> ProviderConfig {
        ProviderConfig {
            model: "claude-test".to_owned(),
            api_key: Some("anthropic-key".to_owned()),
            timeout_seconds: 5,
            max_tokens: 100,
            max_attempts: 1,
            ..ProviderConfig::for_provider(LlmProvider::Anthropic)
        }
    }

    #[test]
    fn messages_contract_omits_temperature_and_keeps_the_legacy_shape() {
        let payload = build_payload(
            &config(),
            &CompletionRequest {
                system: "system",
                stable_context: None,
                user_text: "read",
                image_png_base64: Some("cG5n"),
                response_format: Some(&json!({ "type": "json_object" })),
                previous_response: None,
                correction: None,
            },
        );

        assert_eq!(payload["system"], "system");
        assert!(payload.get("temperature").is_none());
        assert_eq!(
            payload.pointer("/messages/0/content/1/source/data"),
            Some(&json!("cG5n"))
        );

        let instruction = payload
            .pointer("/messages/0/content/0/text")
            .and_then(Value::as_str)
            .expect("instruction text");
        assert!(
            instruction.contains("json_object"),
            "without a stable block the schema stays on the user turn"
        );
    }

    #[test]
    fn a_stable_block_becomes_a_cached_system_prefix() {
        let payload = build_payload(
            &config(),
            &CompletionRequest {
                system: "system",
                stable_context: Some("stage guidance"),
                user_text: "row hint",
                image_png_base64: Some("cG5n"),
                response_format: Some(&json!({ "type": "json_object" })),
                previous_response: None,
                correction: None,
            },
        );

        assert_eq!(payload.pointer("/system/0/text"), Some(&json!("system")));
        assert!(
            payload.pointer("/system/0/cache_control").is_none(),
            "only the last stable block carries the breakpoint"
        );
        assert_eq!(
            payload.pointer("/system/1/cache_control"),
            Some(&json!({ "type": "ephemeral" }))
        );

        let cached = payload
            .pointer("/system/1/text")
            .and_then(Value::as_str)
            .expect("cached block");
        assert!(cached.starts_with("stage guidance"));
        assert!(
            cached.contains("json_object"),
            "the schema is stable and belongs in the cached prefix"
        );

        let instruction = payload
            .pointer("/messages/0/content/0/text")
            .and_then(Value::as_str)
            .expect("instruction text");
        assert_eq!(
            instruction, "row hint",
            "the user turn must carry per-request content only"
        );
    }

    #[test]
    fn retry_turns_stay_behind_the_cached_prefix() {
        let payload = build_payload(
            &config(),
            &CompletionRequest {
                system: "system",
                stable_context: Some("stage guidance"),
                user_text: "row hint",
                image_png_base64: None,
                response_format: None,
                previous_response: Some("{\"bad\":true}"),
                correction: Some("fix it"),
            },
        );

        assert_eq!(
            payload.pointer("/system/1/text"),
            Some(&json!("stage guidance"))
        );
        assert_eq!(payload.pointer("/messages/0/role"), Some(&json!("user")));
        assert_eq!(
            payload.pointer("/messages/1/role"),
            Some(&json!("assistant"))
        );
        assert_eq!(
            payload.pointer("/messages/2/content"),
            Some(&json!("fix it"))
        );
    }

    #[test]
    fn completion_response_is_parsed_from_the_first_text_block() {
        let parsed = parse_completion_response(
            r#"{"model":"claude-test","content":[{"type":"text","text":"{\"ok\":true}"}],"usage":{"input_tokens":4}}"#,
        )
        .expect("Anthropic response");
        assert_eq!(parsed.content, "{\"ok\":true}");
        assert_eq!(parsed.model.as_deref(), Some("claude-test"));
    }
}
