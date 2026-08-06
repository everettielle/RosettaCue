use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::http;
use crate::{CompletionRequest, CompletionResponse, LlmError, LlmModel, ProviderConfig};

pub(super) fn complete(
    client: &Client,
    config: &ProviderConfig,
    request: &CompletionRequest<'_>,
) -> Result<CompletionResponse, LlmError> {
    let payload = build_payload(config, request);
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

fn build_payload(config: &ProviderConfig, request: &CompletionRequest<'_>) -> Value {
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
    use crate::LlmProvider;

    #[test]
    fn vision_contract_preserves_sampling_parameters() {
        let config = ProviderConfig {
            provider: LlmProvider::OpenAi,
            base_url: "https://api.openai.test/v1".to_owned(),
            model: "vision-model".to_owned(),
            api_key: Some("test-key".to_owned()),
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
        assert_eq!(payload["model"], "vision-model");
        assert_eq!(payload["temperature"], 0);
        assert_eq!(payload["seed"], 0);
        assert_eq!(payload["stream"], false);
        assert_eq!(
            payload.pointer("/messages/1/content/1/image_url/url"),
            Some(&json!("data:image/png;base64,cG5n"))
        );
        let parsed = parse_completion_response(
            r#"{"model":"vision-model","choices":[{"message":{"content":"{\"ok\":true}"}}],"usage":{"total_tokens":7}}"#,
        )
        .expect("OpenAI-compatible response");
        assert_eq!(parsed.content, "{\"ok\":true}");
    }
}
