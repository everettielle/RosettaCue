use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

static ENABLED: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(1);
static SINK: OnceLock<Arc<dyn Fn(DiagnosticEntry) + Send + Sync>> = OnceLock::new();
static SECRETS: OnceLock<RwLock<Vec<String>>> = OnceLock::new();

thread_local! {
    static CORRELATION_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub id: String,
    pub sequence: u64,
    pub created_at_ms: u64,
    pub level: DiagnosticLevel,
    pub source: String,
    pub category: String,
    pub operation: String,
    pub phase: String,
    pub message: String,
    pub correlation_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub details: Value,
}

#[derive(Debug, Clone)]
pub struct DiagnosticEvent<'a> {
    pub level: DiagnosticLevel,
    pub source: &'a str,
    pub category: &'a str,
    pub operation: &'a str,
    pub phase: &'a str,
    pub message: &'a str,
    pub duration_ms: Option<u64>,
    pub details: Value,
}

#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn configure(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn register_secret(secret: &str) {
    if secret.is_empty() {
        return;
    }
    let secrets = SECRETS.get_or_init(|| RwLock::new(Vec::new()));
    let mut values = secrets
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !values.iter().any(|value| value == secret) {
        values.push(secret.to_owned());
    }
}

/// Registers the process-wide diagnostic destination.
///
/// # Errors
///
/// Returns the supplied sink when a destination has already been installed.
pub fn set_sink(
    sink: Arc<dyn Fn(DiagnosticEntry) + Send + Sync>,
) -> Result<(), Arc<dyn Fn(DiagnosticEntry) + Send + Sync>> {
    SINK.set(sink)
}

pub fn emit(event: DiagnosticEvent<'_>) {
    if !enabled() {
        return;
    }
    let Some(sink) = SINK.get() else {
        return;
    };
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let created_at_ms = now_ms();
    let correlation_id = CORRELATION_ID.with(|value| value.borrow().clone());
    sink(DiagnosticEntry {
        id: format!("{}-{created_at_ms}-{sequence}", std::process::id()),
        sequence,
        created_at_ms,
        level: event.level,
        source: event.source.to_owned(),
        category: event.category.to_owned(),
        operation: event.operation.to_owned(),
        phase: event.phase.to_owned(),
        message: event.message.to_owned(),
        correlation_id,
        duration_ms: event.duration_ms,
        details: redact_json(event.details),
    });
}

pub fn with_correlation<T>(correlation_id: impl Into<String>, operation: impl FnOnce() -> T) -> T {
    let correlation_id = correlation_id.into();
    let previous = CORRELATION_ID.with(|value| value.replace(Some(correlation_id)));
    let result = operation();
    CORRELATION_ID.with(|value| {
        value.replace(previous);
    });
    result
}

#[must_use]
pub fn redact_json(mut value: Value) -> Value {
    redact_value(&mut value, None);
    value
}

fn redact_value(value: &mut Value, key: Option<&str>) {
    if key.is_some_and(is_secret_key) {
        *value = Value::String("[REDACTED]".to_owned());
        return;
    }
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                redact_value(value, Some(key));
            }
        }
        Value::Array(array) => {
            for value in array {
                redact_value(value, key);
            }
        }
        Value::String(text) => {
            if text.starts_with("data:") && text.contains(";base64,") {
                let byte_length = text
                    .split_once(";base64,")
                    .map_or(0, |(_, encoded)| encoded.len().saturating_mul(3) / 4);
                *value = serde_json::json!({
                    "redacted": "base64_data",
                    "estimated_byte_length": byte_length
                });
                return;
            }
            if let Some(secrets) = SECRETS.get() {
                let secrets = secrets
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                for secret in secrets.iter() {
                    if text.contains(secret) {
                        *text = text.replace(secret, "[REDACTED]");
                    }
                }
            }
        }
        _ => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().replace('-', "_").as_str(),
        "api_key"
            | "apikey"
            | "authorization"
            | "x_api_key"
            | "access_token"
            | "refresh_token"
            | "password"
            | "cookie"
            | "set_cookie"
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_nested_credentials_and_base64_images() {
        let value = redact_json(serde_json::json!({
            "api_key": "secret",
            "headers": { "Authorization": "Bearer secret" },
            "image": "data:image/png;base64,cG5n"
        }));
        assert_eq!(value["api_key"], "[REDACTED]");
        assert_eq!(value["headers"]["Authorization"], "[REDACTED]");
        assert_eq!(value["image"]["redacted"], "base64_data");
    }

    #[test]
    fn redacts_registered_secrets_inside_response_bodies() {
        register_secret("test-secret-value");
        let value = redact_json(Value::String(
            "provider echoed test-secret-value unexpectedly".to_owned(),
        ));
        assert_eq!(value, "provider echoed [REDACTED] unexpectedly");
    }
}
