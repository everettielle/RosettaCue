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

fn build_payload(config: &ProviderConfig, request: &CompletionRequest<'_>) -> Value {
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
    json!({
        "model": config.model,
        "system": request.system,
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

    #[test]
    fn messages_contract_omits_temperature() {
        let config = ProviderConfig {
            provider: LlmProvider::Anthropic,
            base_url: "https://api.anthropic.test/v1".to_owned(),
            model: "claude-test".to_owned(),
            api_key: Some("anthropic-key".to_owned()),
            timeout_seconds: 5,
            max_tokens: 100,
            max_attempts: 1,
        };
        let payload = build_payload(
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
        assert!(payload.get("temperature").is_none());
        assert_eq!(
            payload.pointer("/messages/0/content/1/source/data"),
            Some(&json!("cG5n"))
        );
        let parsed = parse_completion_response(
            r#"{"model":"claude-test","content":[{"type":"text","text":"{\"ok\":true}"}],"usage":{"input_tokens":4}}"#,
        )
        .expect("Anthropic response");
        assert_eq!(parsed.model.as_deref(), Some("claude-test"));
    }
}
