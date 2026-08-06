use reqwest::blocking::{RequestBuilder, Response};
use rosettacue_diagnostics::{DiagnosticEvent, DiagnosticLevel};
use serde_json::Value;

use crate::{LlmError, LlmModel, ProviderConfig};

pub(super) fn authenticate_bearer(
    request: RequestBuilder,
    api_key: Option<&str>,
) -> RequestBuilder {
    if let Some(key) = api_key.filter(|key| !key.is_empty()) {
        request.bearer_auth(key)
    } else {
        request
    }
}

pub(super) fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path)
}

pub(super) fn parse_response<T>(
    response: Response,
    operation: &str,
    parser: impl FnOnce(&str) -> Result<T, LlmError>,
) -> Result<T, LlmError> {
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .filter(|(name, _)| is_diagnostic_header(name.as_str()))
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), Value::String(value.to_owned())))
        })
        .collect::<serde_json::Map<_, _>>();
    let body = response.text().map_err(map_request_error)?;
    parse_response_parts(
        status.is_success(),
        status.as_u16(),
        &headers,
        &body,
        operation,
        parser,
    )
}

fn parse_response_parts<T>(
    success: bool,
    status: u16,
    headers: &serde_json::Map<String, Value>,
    body: &str,
    operation: &str,
    parser: impl FnOnce(&str) -> Result<T, LlmError>,
) -> Result<T, LlmError> {
    if !success {
        emit_response(
            operation,
            DiagnosticLevel::Error,
            "failed",
            status,
            headers,
            body,
            Some("Provider returned a non-success HTTP status."),
        );
        return Err(LlmError::Http {
            status,
            message: body.chars().take(1_000).collect(),
        });
    }
    let result = parser(body);
    emit_response(
        operation,
        if result.is_ok() {
            DiagnosticLevel::Debug
        } else {
            DiagnosticLevel::Error
        },
        if result.is_ok() {
            "completed"
        } else {
            "invalid"
        },
        status,
        headers,
        body,
        result.as_ref().err().map(ToString::to_string).as_deref(),
    );
    result
}

pub(super) fn emit_request(
    config: &ProviderConfig,
    operation: &str,
    method: &str,
    endpoint: &str,
    body: Option<&Value>,
) {
    if !rosettacue_diagnostics::enabled() {
        return;
    }
    rosettacue_diagnostics::emit(DiagnosticEvent {
        level: DiagnosticLevel::Debug,
        source: "llm",
        category: "http",
        operation,
        phase: "request",
        message: "Sending provider HTTP request.",
        duration_ms: None,
        details: serde_json::json!({
            "provider": config.provider.as_str(),
            "model": config.model,
            "request": {
                "method": method,
                "url": sanitized_url(endpoint),
                "body": body
            }
        }),
    });
}

pub(super) fn emit_transport_error(
    config: &ProviderConfig,
    operation: &str,
    error: &reqwest::Error,
) {
    if !rosettacue_diagnostics::enabled() {
        return;
    }
    rosettacue_diagnostics::emit(DiagnosticEvent {
        level: DiagnosticLevel::Error,
        source: "llm",
        category: "http",
        operation,
        phase: "transport_failed",
        message: "Provider HTTP request failed before a response was received.",
        duration_ms: None,
        details: serde_json::json!({
            "provider": config.provider.as_str(),
            "model": config.model,
            "error": error.to_string(),
            "is_connect": error.is_connect(),
            "is_timeout": error.is_timeout()
        }),
    });
}

fn emit_response(
    operation: &str,
    level: DiagnosticLevel,
    phase: &str,
    status: u16,
    headers: &serde_json::Map<String, Value>,
    body: &str,
    error: Option<&str>,
) {
    if !rosettacue_diagnostics::enabled() {
        return;
    }
    rosettacue_diagnostics::emit(DiagnosticEvent {
        level,
        source: "llm",
        category: "http",
        operation,
        phase,
        message: "Received provider HTTP response.",
        duration_ms: None,
        details: serde_json::json!({
            "response": {
                "status": status,
                "headers": headers,
                "body": body
            },
            "error": error
        }),
    });
}

fn is_diagnostic_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "content-type"
        || name == "content-length"
        || name == "retry-after"
        || name.contains("request-id")
        || name.starts_with("x-ratelimit-")
        || name.starts_with("anthropic-ratelimit-")
}

fn sanitized_url(endpoint: &str) -> String {
    reqwest::Url::parse(endpoint).map_or_else(
        |_| endpoint.to_owned(),
        |mut url| {
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        },
    )
}

pub(super) fn parse_model_list(body: &str) -> Result<Vec<LlmModel>, LlmError> {
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
}

pub(super) fn map_request_error(error: reqwest::Error) -> LlmError {
    if error.is_connect() || error.is_timeout() {
        LlmError::Unavailable(error.to_string())
    } else {
        LlmError::Request(error)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn diagnostics_preserve_the_complete_provider_response_envelope() {
        let entries = Arc::new(Mutex::new(Vec::new()));
        let captured = entries.clone();
        let _ = rosettacue_diagnostics::set_sink(Arc::new(move |entry| {
            captured.lock().expect("diagnostic entries").push(entry);
        }));
        rosettacue_diagnostics::configure(true);

        let body =
            r#"{"choices":[{"message":{"content":"","reasoning_content":"{\\"lines\\":[]}"}}]}"#;
        let parsed = parse_response_parts(
            true,
            200,
            &serde_json::Map::from_iter([(
                "content-type".to_owned(),
                Value::String("application/json".to_owned()),
            )]),
            body,
            "completion",
            |value| Ok(value.to_owned()),
        )
        .expect("parsed response");
        rosettacue_diagnostics::configure(false);

        assert_eq!(parsed, body);
        let entries = entries.lock().expect("diagnostic entries");
        let response = entries
            .iter()
            .find(|entry| entry.operation == "completion" && entry.phase == "completed")
            .expect("response diagnostic");
        assert_eq!(response.details["response"]["body"], body);
    }
}
