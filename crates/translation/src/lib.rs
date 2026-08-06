use std::time::Instant;

use rosettacue_diagnostics::{DiagnosticEvent, DiagnosticLevel};
use rosettacue_domain::{CueEditDocument, OcrDocument, OcrLine, OcrSpan, TextStyle};
use rosettacue_llm::{CompletionRequest, ProviderClient, ProviderConfig};
use serde::Deserialize;
use serde_json::{Value, json};

pub const TRANSLATION_PROMPT_VERSION: &str = "subtitle-translation-v1";

const SYSTEM_PROMPT: &str = r"You are a professional audiovisual subtitle translator.
Translate only the supplied subtitle lines into the requested target language.
Preserve meaning, tone, speaker intent, punctuation, and deliberate ellipses.
Use surrounding cues only as context; never include their translations in the output.
Return exactly one output item for every input line, with the same 1-based line_index.
Do not add commentary, notes, Markdown, speaker labels, or explanations.";

#[derive(Debug, Clone)]
pub struct TranslationRequest<'a> {
    pub document: &'a CueEditDocument,
    pub source_language: &'a str,
    pub target_language: &'a str,
    pub previous_context: Option<&'a str>,
    pub next_context: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct TranslationOutput {
    pub document: CueEditDocument,
    pub raw_response: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Deserialize)]
struct TranslationResponse {
    lines: Vec<TranslatedLine>,
}

#[derive(Debug, Deserialize)]
struct TranslatedLine {
    line_index: u32,
    text: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TranslationError {
    #[error("translation target language is required")]
    MissingTargetLanguage,
    #[error("subtitle has no lines to translate")]
    EmptySource,
    #[error("translation response failed validation: {0}")]
    Validation(String),
    #[error(transparent)]
    Provider(#[from] rosettacue_llm::LlmError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub struct SubtitleTranslator {
    client: ProviderClient,
}

impl SubtitleTranslator {
    /// Creates a translator backed by any supported LLM provider.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider configuration is invalid.
    pub fn new(config: ProviderConfig) -> Result<Self, TranslationError> {
        Ok(Self {
            client: ProviderClient::new(config)?,
        })
    }

    /// Translates one cue while retaining its timing, position, and uniform span styles.
    ///
    /// # Errors
    ///
    /// Returns an error for empty input, provider failures, or an invalid line mapping.
    #[allow(clippy::too_many_lines)]
    pub fn translate(
        &self,
        request: &TranslationRequest<'_>,
    ) -> Result<TranslationOutput, TranslationError> {
        if request.target_language.trim().is_empty() {
            return Err(TranslationError::MissingTargetLanguage);
        }
        if request.document.subtitle.lines.is_empty() {
            return Err(TranslationError::EmptySource);
        }
        let started = Instant::now();
        let user_text = translation_prompt(request);
        let schema = translation_schema();
        let mut previous_response: Option<String> = None;
        let mut correction: Option<String> = None;
        for attempt in 1..=self.client.config().max_attempts {
            translation_event(
                "translate",
                "attempt",
                DiagnosticLevel::Debug,
                "Translation attempt started.",
                None,
                || {
                    serde_json::json!({
                        "attempt": attempt,
                        "provider": self.client.config().provider.as_str(),
                        "model": self.client.config().model,
                        "source_language": request.source_language,
                        "target_language": request.target_language,
                        "line_count": request.document.subtitle.lines.len()
                    })
                },
            );
            let response = self.client.complete(&CompletionRequest {
                system: SYSTEM_PROMPT,
                user_text: &user_text,
                image_png_base64: None,
                response_format: Some(&schema),
                previous_response: previous_response.as_deref(),
                correction: correction.as_deref(),
            })?;
            match validate_response(&response.content, request.document.subtitle.lines.len()) {
                Ok(lines) => {
                    let document = translated_document(
                        request.document,
                        lines,
                        self.client.config(),
                        request.target_language,
                    );
                    let elapsed_ms =
                        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    translation_event(
                        "translate",
                        "succeeded",
                        DiagnosticLevel::Info,
                        "Translation response validated.",
                        Some(elapsed_ms),
                        || {
                            serde_json::json!({
                                "attempt": attempt,
                                "candidate_content": response.content,
                                "line_count": document.subtitle.lines.len()
                            })
                        },
                    );
                    return Ok(TranslationOutput {
                        document,
                        raw_response: response.content,
                        elapsed_ms,
                    });
                }
                Err(error) if attempt < self.client.config().max_attempts => {
                    translation_event(
                        "translate",
                        "validation_failed",
                        DiagnosticLevel::Warn,
                        "Translation response validation failed; retrying.",
                        None,
                        || {
                            serde_json::json!({
                                "attempt": attempt,
                                "candidate_content": response.content,
                                "error": error.to_string()
                            })
                        },
                    );
                    correction = Some(format!(
                        "The previous JSON was invalid: {error}. Return the complete corrected JSON only."
                    ));
                    previous_response = Some(response.content);
                }
                Err(error) => {
                    translation_event(
                        "translate",
                        "validation_failed",
                        DiagnosticLevel::Error,
                        "Translation response validation failed.",
                        None,
                        || {
                            serde_json::json!({
                                "attempt": attempt,
                                "candidate_content": response.content,
                                "error": error.to_string()
                            })
                        },
                    );
                    return Err(error);
                }
            }
        }
        Err(TranslationError::Validation(
            "all translation attempts were exhausted".to_owned(),
        ))
    }
}

fn translation_event(
    operation: &str,
    phase: &str,
    level: DiagnosticLevel,
    message: &str,
    duration_ms: Option<u64>,
    details: impl FnOnce() -> serde_json::Value,
) {
    if !rosettacue_diagnostics::enabled() {
        return;
    }
    rosettacue_diagnostics::emit(DiagnosticEvent {
        level,
        source: "translation",
        category: "pipeline",
        operation,
        phase,
        message,
        duration_ms,
        details: details(),
    });
}

fn translation_prompt(request: &TranslationRequest<'_>) -> String {
    let lines = request
        .document
        .subtitle
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| json!({ "line_index": index + 1, "text": line.text }))
        .collect::<Vec<_>>();
    json!({
        "task": "translate_subtitle_cue",
        "source_language": request.source_language,
        "target_language": request.target_language,
        "previous_cue_context": request.previous_context,
        "next_cue_context": request.next_context,
        "lines": lines
    })
    .to_string()
}

fn translation_schema() -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "subtitle_translation",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "lines": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "line_index": { "type": "integer", "minimum": 1 },
                                "text": { "type": "string", "minLength": 1 }
                            },
                            "required": ["line_index", "text"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["lines"],
                "additionalProperties": false
            }
        }
    })
}

fn validate_response(content: &str, line_count: usize) -> Result<Vec<String>, TranslationError> {
    let parsed = parse_json_content(content)?;
    let response = serde_json::from_value::<TranslationResponse>(parsed)?;
    if response.lines.len() != line_count {
        return Err(TranslationError::Validation(format!(
            "expected {line_count} lines, received {}",
            response.lines.len()
        )));
    }
    let mut ordered = vec![None; line_count];
    for line in response.lines {
        let index = usize::try_from(line.line_index)
            .ok()
            .and_then(|value| value.checked_sub(1))
            .filter(|index| *index < line_count)
            .ok_or_else(|| TranslationError::Validation("line index is out of range".to_owned()))?;
        let text = line.text.trim().to_owned();
        if text.is_empty() || ordered[index].replace(text).is_some() {
            return Err(TranslationError::Validation(
                "line mapping is empty or duplicated".to_owned(),
            ));
        }
    }
    ordered
        .into_iter()
        .map(|line| {
            line.ok_or_else(|| {
                TranslationError::Validation("line mapping is incomplete".to_owned())
            })
        })
        .collect()
}

fn parse_json_content(content: &str) -> Result<Value, TranslationError> {
    let stripped = content.trim();
    let stripped = if stripped.starts_with("```") {
        stripped
            .lines()
            .skip(1)
            .take_while(|line| line.trim() != "```")
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        stripped.to_owned()
    };
    serde_json::from_str(&stripped)
        .map_err(|_| TranslationError::Validation("provider did not return valid JSON".to_owned()))
}

fn translated_document(
    source: &CueEditDocument,
    lines: Vec<String>,
    config: &ProviderConfig,
    target_language: &str,
) -> CueEditDocument {
    let translated_lines = source
        .subtitle
        .lines
        .iter()
        .zip(lines)
        .map(|(source_line, text)| {
            let styles = uniform_line_styles(source_line);
            let color = uniform_line_color(source_line);
            OcrLine {
                spans: vec![OcrSpan::Text {
                    text: text.clone(),
                    styles,
                    color,
                }],
                text,
            }
        })
        .collect();
    CueEditDocument {
        start_ms: source.start_ms,
        end_ms: source.end_ms,
        position: source.position,
        subtitle: OcrDocument {
            prompt_version: TRANSLATION_PROMPT_VERSION.to_owned(),
            provider: config.provider.as_str().to_owned(),
            model: config.model.clone(),
            language: target_language.to_owned(),
            unreadable: false,
            lines: translated_lines,
            normalizations: Vec::new(),
        },
    }
}

fn uniform_line_styles(line: &OcrLine) -> Vec<TextStyle> {
    let Some(first) = line.spans.first().map(OcrSpan::styles) else {
        return Vec::new();
    };
    if line.spans.iter().all(|span| span.styles() == first) {
        first.to_vec()
    } else {
        Vec::new()
    }
}

fn uniform_line_color(line: &OcrLine) -> Option<String> {
    let first = line.spans.first()?.color();
    if line.spans.iter().all(|span| span.color() == first) {
        first.map(str::to_owned)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use rosettacue_domain::{SubtitlePosition, TextStyle};

    use super::*;

    fn source() -> CueEditDocument {
        CueEditDocument {
            start_ms: 100,
            end_ms: 900,
            position: SubtitlePosition::BottomCenter,
            subtitle: OcrDocument {
                prompt_version: "ocr".to_owned(),
                provider: "lmstudio".to_owned(),
                model: "vision".to_owned(),
                language: "jpn".to_owned(),
                unreadable: false,
                lines: vec![OcrLine {
                    text: "物語は続く。".to_owned(),
                    spans: vec![OcrSpan::Text {
                        text: "物語は続く。".to_owned(),
                        styles: vec![TextStyle::Italic],
                        color: Some("#FFFF00".to_owned()),
                    }],
                }],
                normalizations: Vec::new(),
            },
        }
    }

    #[test]
    fn validates_line_mapping_and_preserves_layout_style() {
        let lines = validate_response(
            r#"{"lines":[{"line_index":1,"text":"The story continues."}]}"#,
            1,
        )
        .expect("translation response");
        let config = ProviderConfig {
            model: "translator".to_owned(),
            ..ProviderConfig::default()
        };
        let translated = translated_document(&source(), lines, &config, "eng");
        assert_eq!(translated.start_ms, 100);
        assert_eq!(translated.subtitle.language, "eng");
        assert_eq!(
            translated.subtitle.lines[0].spans[0].styles(),
            &[TextStyle::Italic]
        );
        assert_eq!(
            translated.subtitle.lines[0].spans[0].color(),
            Some("#FFFF00")
        );
        assert_eq!(translated.subtitle.lines[0].text, "The story continues.");
    }

    #[test]
    fn rejects_missing_or_duplicate_lines() {
        assert!(validate_response(r#"{"lines":[]}"#, 1).is_err());
        assert!(
            validate_response(
                r#"{"lines":[{"line_index":1,"text":"A"},{"line_index":1,"text":"B"}]}"#,
                2,
            )
            .is_err()
        );
    }
}
