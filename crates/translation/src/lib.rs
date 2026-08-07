use std::time::Instant;

use rosettacue_diagnostics::{DiagnosticEvent, DiagnosticLevel};
use rosettacue_domain::{
    CueEditDocument, OcrDocument, OcrLine, OcrSpan, ProperNounMapping, TextBlock, TextStyle,
};
use rosettacue_llm::{CompletionRequest, ProviderClient, ProviderConfig};
use serde::Deserialize;
use serde_json::{Value, json};

pub const TRANSLATION_PROMPT_VERSION: &str = "subtitle-translation-v2";

const SYSTEM_PROMPT: &str = r"You are a professional audiovisual subtitle translator.
Translate only the supplied subtitle lines into the requested target language.
Preserve meaning, tone, speaker intent, punctuation, and deliberate ellipses.
Use surrounding cues only as context; never include their translations in the output.
When proper_noun_mappings are supplied, use each mapped translation exactly whenever its source proper noun occurs. Do not translate, inflect, or annotate the mapped form differently.
Return exactly one output item for every input line, with the same 1-based line_index.
Do not add commentary, notes, Markdown, speaker labels, or explanations.";

#[derive(Debug, Clone)]
pub struct TranslationRequest<'a> {
    pub document: &'a CueEditDocument,
    pub source_language: &'a str,
    pub target_language: &'a str,
    pub previous_context: Option<&'a str>,
    pub next_context: Option<&'a str>,
    pub proper_nouns: &'a [ProperNounMapping],
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
        if request.document.subtitle.line_count() == 0 {
            return Err(TranslationError::EmptySource);
        }
        let started = Instant::now();
        let stable_context = translation_context(request);
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
                        "line_count": request.document.subtitle.line_count(),
                        "proper_noun_count": request.proper_nouns.len()
                    })
                },
            );
            let response = self.client.complete(&CompletionRequest {
                system: SYSTEM_PROMPT,
                stable_context: Some(&stable_context),
                user_text: &user_text,
                image_png_base64: None,
                response_format: Some(&schema),
                previous_response: previous_response.as_deref(),
                correction: correction.as_deref(),
            })?;
            match validate_response(&response.content, request.document.subtitle.line_count()) {
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
                                "line_count": document.subtitle.line_count()
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

/// Job-scoped translation context.
///
/// Identical for every Cue in one translation run — the language pair and the
/// project's proper-noun glossary — so it forms the cacheable prefix. The
/// glossary is the reason this matters: it is re-sent on every Cue and grows
/// with the project.
fn translation_context(request: &TranslationRequest<'_>) -> String {
    json!({
        "task": "translate_subtitle_cue",
        "source_language": request.source_language,
        "target_language": request.target_language,
        "proper_noun_mappings": request.proper_nouns
    })
    .to_string()
}

fn translation_prompt(request: &TranslationRequest<'_>) -> String {
    let lines = request
        .document
        .subtitle
        .lines()
        .enumerate()
        .map(|(index, line)| json!({ "line_index": index + 1, "text": line.text }))
        .collect::<Vec<_>>();
    json!({
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
    // Placement, direction, and provenance are facts about the source bitmap,
    // so translation carries them through untouched and replaces only text.
    let mut translated = lines.into_iter();
    let blocks = source
        .subtitle
        .blocks
        .iter()
        .map(|block| TextBlock {
            bounds: block.bounds,
            writing_mode: block.writing_mode,
            position: block.position,
            source: block.source,
            lines: block
                .lines
                .iter()
                .zip(translated.by_ref())
                .map(|(source_line, text)| translated_line(source_line, text))
                .collect(),
        })
        .collect();
    CueEditDocument {
        start_ms: source.start_ms,
        end_ms: source.end_ms,
        subtitle: OcrDocument {
            prompt_version: TRANSLATION_PROMPT_VERSION.to_owned(),
            provider: config.provider.as_str().to_owned(),
            model: config.model.clone(),
            language: target_language.to_owned(),
            unreadable: false,
            blocks,
            normalizations: Vec::new(),
        },
    }
}

fn translated_line(source: &OcrLine, text: String) -> OcrLine {
    let styles = uniform_line_styles(source);
    let color = uniform_line_color(source);
    OcrLine {
        spans: vec![OcrSpan::Text {
            text: text.clone(),
            styles,
            color,
        }],
        text,
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
    use rosettacue_domain::{BlockBounds, BlockSource, SubtitlePosition, TextStyle, WritingMode};

    use super::*;

    fn block(writing_mode: WritingMode, lines: Vec<OcrLine>) -> TextBlock {
        TextBlock {
            bounds: BlockBounds {
                x: 20,
                y: 900,
                width: 400,
                height: 60,
            },
            writing_mode,
            position: SubtitlePosition::BottomCenter,
            source: BlockSource::Detected,
            lines,
        }
    }

    fn line(text: &str) -> OcrLine {
        OcrLine {
            text: text.to_owned(),
            spans: vec![OcrSpan::Text {
                text: text.to_owned(),
                styles: vec![TextStyle::Italic],
                color: Some("#FFFF00".to_owned()),
            }],
        }
    }

    fn source() -> CueEditDocument {
        CueEditDocument {
            start_ms: 100,
            end_ms: 900,
            subtitle: OcrDocument {
                prompt_version: "ocr".to_owned(),
                provider: "lmstudio".to_owned(),
                model: "vision".to_owned(),
                language: "jpn".to_owned(),
                unreadable: false,
                blocks: vec![block(WritingMode::HorizontalTb, vec![line("物語は続く。")])],
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
        let first = translated
            .subtitle
            .lines()
            .next()
            .expect("a translated line");
        assert_eq!(first.spans[0].styles(), &[TextStyle::Italic]);
        assert_eq!(first.spans[0].color(), Some("#FFFF00"));
        assert_eq!(first.text, "The story continues.");
    }

    #[test]
    fn keeps_every_block_boundary_placement_and_direction() {
        let mut document = source();
        document.subtitle.blocks = vec![
            block(WritingMode::VerticalRl, vec![line("冷たい！")]),
            block(
                WritingMode::HorizontalTb,
                vec![line("ながく"), line("おもいけるかな")],
            ),
        ];
        let lines = validate_response(
            r#"{"lines":[{"line_index":1,"text":"Cold!"},{"line_index":2,"text":"Long"},{"line_index":3,"text":"I wished"}]}"#,
            3,
        )
        .expect("translation response");

        let translated = translated_document(&document, lines, &ProviderConfig::default(), "eng");

        assert_eq!(translated.subtitle.blocks.len(), 2);
        assert_eq!(
            translated.subtitle.blocks[0].writing_mode,
            WritingMode::VerticalRl,
            "writing mode is a fact about the source bitmap, not the text"
        );
        assert_eq!(translated.subtitle.blocks[0].lines.len(), 1);
        assert_eq!(translated.subtitle.blocks[1].lines.len(), 2);
        assert_eq!(translated.subtitle.blocks[1].lines[1].text, "I wished");
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

    #[test]
    fn includes_project_proper_noun_mappings_in_the_cached_context() {
        let mappings = vec![ProperNounMapping {
            source: "綾瀬千早".to_owned(),
            translation: "Chihaya Ayase".to_owned(),
        }];
        let request = TranslationRequest {
            document: &source(),
            source_language: "jpn",
            target_language: "eng",
            previous_context: None,
            next_context: None,
            proper_nouns: &mappings,
        };
        let context: Value = serde_json::from_str(&translation_context(&request)).expect("context");
        let prompt: Value = serde_json::from_str(&translation_prompt(&request)).expect("prompt");

        assert_eq!(
            context["proper_noun_mappings"],
            json!([{"source":"綾瀬千早","translation":"Chihaya Ayase"}])
        );
        assert_eq!(context["source_language"], "jpn");
        assert_eq!(context["target_language"], "eng");
        assert!(
            prompt.get("proper_noun_mappings").is_none(),
            "the glossary is job-scoped and belongs in the cached prefix"
        );
    }

    #[test]
    fn the_cached_context_is_stable_across_cues_in_one_job() {
        let mappings = vec![ProperNounMapping {
            source: "綾瀬千早".to_owned(),
            translation: "Chihaya Ayase".to_owned(),
        }];
        let document = source();
        let request = |previous, next| TranslationRequest {
            document: &document,
            source_language: "jpn",
            target_language: "eng",
            previous_context: previous,
            next_context: next,
            proper_nouns: &mappings,
        };

        // Neighbouring-cue context changes per Cue and must not disturb the prefix.
        let first = request(None, Some("次の台詞"));
        let second = request(Some("前の台詞"), None);
        assert_eq!(
            translation_context(&first),
            translation_context(&second),
            "the cached prefix must not move between Cues"
        );
        assert_ne!(translation_prompt(&first), translation_prompt(&second));

        let prompt: Value = serde_json::from_str(&translation_prompt(&first)).expect("prompt");
        assert_eq!(prompt["next_cue_context"], "次の台詞");
        assert!(prompt["lines"].is_array());
    }
}
