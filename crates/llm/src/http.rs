use reqwest::blocking::{RequestBuilder, Response};
use serde_json::Value;

use crate::{LlmError, LlmModel};

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
