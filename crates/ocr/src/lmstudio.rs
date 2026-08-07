use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rosettacue_diagnostics::{DiagnosticEvent, DiagnosticLevel};
use rosettacue_domain::{
    NormalizationRecord, OcrDocument, OcrLine, OcrSpan, RubyAnnotation, RubyPosition, TextStyle,
};
use rosettacue_llm::{
    CompletionRequest, CompletionResponse, LlmModel, LlmProvider, ProviderClient, ProviderConfig,
    ProviderDiagnostic,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::languages::{self, LanguagePreset, NormalizationEvent};
use crate::prompt::{self, PROMPT_VERSION, SYSTEM_PROMPT, StagePrompt};
use crate::row_detection::estimate_main_rows;
use crate::{OcrBackend, OcrError, OcrRecognition, OcrRequest};

pub type LmStudioConfig = ProviderConfig;
pub type LmStudioModel = LlmModel;
pub type LmStudioBackend = ProviderOcrBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrPipelineConfig {
    pub recognition: ProviderConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruby: Option<ProviderConfig>,
    pub validation: ProviderConfig,
}

impl OcrPipelineConfig {
    #[must_use]
    pub fn single(config: ProviderConfig) -> Self {
        Self {
            recognition: config.clone(),
            ruby: None,
            validation: config,
        }
    }

    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            recognition: self.recognition.redacted(),
            ruby: self.ruby.as_ref().map(ProviderConfig::redacted),
            validation: self.validation.redacted(),
        }
    }
}

impl Default for OcrPipelineConfig {
    fn default() -> Self {
        Self::single(ProviderConfig::default())
    }
}

pub struct ProviderOcrBackend {
    config: OcrPipelineConfig,
    recognition_client: ProviderClient,
    ruby_client: Option<ProviderClient>,
    validation_client: ProviderClient,
}

#[derive(Debug, Deserialize)]
struct RecognitionResponse {
    lines: Vec<TextValue>,
    annotations: Vec<RawAnnotation>,
    unreadable: bool,
}

#[derive(Debug, Deserialize)]
struct MainTextResponse {
    lines: Vec<TextValue>,
    unreadable: bool,
}

#[derive(Debug, Deserialize)]
struct RubyResponse {
    annotations: Vec<RawAnnotation>,
    unreadable: bool,
}

#[derive(Debug, Deserialize)]
struct TextValue {
    text: String,
}

#[derive(Debug, Deserialize)]
struct RawAnnotation {
    line_index: u32,
    text: String,
    base: String,
    base_occurrence: u32,
    position: String,
}

#[derive(Debug, Clone)]
struct ValidatedAnnotation {
    line_index: u32,
    text: String,
    base: String,
    base_occurrence: u32,
    position: RubyPosition,
}

#[derive(Debug, Deserialize)]
struct StyleResponse {
    italic: bool,
    color: String,
}

struct CharacterStageResult {
    recognition: RecognitionResponse,
    response: CompletionResponse,
    combined: Option<Value>,
    main_text: Option<Value>,
    ruby: Option<Value>,
    usage: Value,
    ruby_usage: Option<Value>,
    mode: &'static str,
}

impl ProviderOcrBackend {
    /// Creates a validated OCR backend using one provider for every pass.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint, model, or numeric limits are invalid.
    pub fn new(config: ProviderConfig) -> Result<Self, OcrError> {
        Self::with_pipeline(OcrPipelineConfig::single(config))
    }

    /// Creates an OCR backend with independent text, optional ruby, and style providers.
    ///
    /// # Errors
    ///
    /// Returns an error when any configured provider is invalid.
    pub fn with_pipeline(config: OcrPipelineConfig) -> Result<Self, OcrError> {
        let recognition_client = ProviderClient::new(config.recognition.clone())?;
        let ruby_client = config
            .ruby
            .as_ref()
            .map(|ruby| ProviderClient::new(ruby.clone()))
            .transpose()?;
        let validation_client = ProviderClient::new(config.validation.clone())?;
        Ok(Self {
            config,
            recognition_client,
            ruby_client,
            validation_client,
        })
    }

    fn run_stage<T>(
        client: &ProviderClient,
        image_url: &str,
        stage: &'static str,
        prompt: &StagePrompt,
        schema: &Value,
        request: &OcrRequest,
        validate: impl Fn(&str) -> Result<T, OcrError>,
    ) -> Result<(T, CompletionResponse, u32), OcrError> {
        let mut previous: Option<String> = None;
        let mut validation_error: Option<String> = None;
        for attempt in 1..=client.config().max_attempts {
            let correction = validation_error
                .as_deref()
                .map(|error| prompt::retry(stage, error));
            let response = match client.complete(&CompletionRequest {
                system: SYSTEM_PROMPT,
                stable_context: Some(&prompt.stable),
                user_text: &prompt.variable,
                image_png_base64: Some(image_url),
                response_format: Some(schema),
                previous_response: previous.as_deref(),
                correction: correction.as_deref(),
            }) {
                Ok(response) => response,
                Err(error) => {
                    emit_stage_event(
                        request,
                        client,
                        stage,
                        attempt,
                        DiagnosticLevel::Error,
                        "provider_failed",
                        None,
                        None,
                        Some(&error.to_string()),
                    );
                    return Err(error.into());
                }
            };
            match validate(&response.content) {
                Ok(value) => {
                    emit_stage_event(
                        request,
                        client,
                        stage,
                        attempt,
                        DiagnosticLevel::Debug,
                        "succeeded",
                        Some(&response.content),
                        Some(&response.usage),
                        None,
                    );
                    return Ok((value, response, attempt));
                }
                Err(error) if attempt < client.config().max_attempts => {
                    emit_stage_event(
                        request,
                        client,
                        stage,
                        attempt,
                        DiagnosticLevel::Warn,
                        "validation_failed",
                        Some(&response.content),
                        Some(&response.usage),
                        Some(&error.to_string()),
                    );
                    validation_error = Some(error.to_string());
                    previous = Some(response.content);
                }
                Err(error) => {
                    emit_stage_event(
                        request,
                        client,
                        stage,
                        attempt,
                        DiagnosticLevel::Error,
                        "validation_failed",
                        Some(&response.content),
                        Some(&response.usage),
                        Some(&error.to_string()),
                    );
                    return Err(error);
                }
            }
        }
        Err(OcrError::Validation(format!(
            "{stage} exhausted all attempts"
        )))
    }

    fn recognize_characters(
        &self,
        image_base64: &str,
        expected_main_rows: Option<usize>,
        language: &LanguagePreset,
        request: &OcrRequest,
    ) -> Result<CharacterStageResult, OcrError> {
        if let Some(ruby_client) = &self.ruby_client {
            let (main_text, main_text_response, _) = Self::run_stage(
                &self.recognition_client,
                image_base64,
                "main_text_recognition",
                &prompt::main_text_recognition(language, expected_main_rows),
                &main_text_schema(),
                request,
                |content| validate_main_text(content, expected_main_rows, language),
            )?;
            let normalized_main_lines = normalized_main_lines(&main_text.lines, language)?;
            let (ruby, ruby_response, _) = Self::run_stage(
                ruby_client,
                image_base64,
                "ruby_recognition",
                &prompt::ruby_recognition(language, &normalized_main_lines),
                &ruby_schema(),
                request,
                |content| validate_ruby(content, &normalized_main_lines, language),
            )?;
            let recognition = RecognitionResponse {
                lines: main_text.lines,
                annotations: ruby.annotations,
                unreadable: main_text.unreadable || ruby.unreadable,
            };
            let main_text_stage = parse_json_content(&main_text_response.content)?;
            let ruby_stage = parse_json_content(&ruby_response.content)?;
            let recognition_usage = main_text_response.usage.clone();
            let ruby_usage = Some(ruby_response.usage);
            Ok(CharacterStageResult {
                recognition,
                response: main_text_response,
                combined: None,
                main_text: Some(main_text_stage),
                ruby: Some(ruby_stage),
                usage: recognition_usage,
                ruby_usage,
                mode: "separate_ruby",
            })
        } else {
            let (recognition, response, _) = Self::run_stage(
                &self.recognition_client,
                image_base64,
                "combined_recognition",
                &prompt::combined_recognition(language, expected_main_rows),
                &combined_recognition_schema(),
                request,
                |content| validate_recognition(content, expected_main_rows, language),
            )?;
            let combined_stage = parse_json_content(&response.content)?;
            let recognition_usage = response.usage.clone();
            Ok(CharacterStageResult {
                recognition,
                response,
                combined: Some(combined_stage),
                main_text: None,
                ruby: None,
                usage: recognition_usage,
                ruby_usage: None,
                mode: "combined",
            })
        }
    }

    fn recognize_inner(&self, request: &OcrRequest) -> Result<OcrRecognition, OcrError> {
        let language = languages::resolve(&request.language)?;
        let started = Instant::now();
        let image = std::fs::read(&request.image_path)?;
        let expected_main_rows = estimate_main_rows(&image);
        let image_base64 = BASE64.encode(image);
        let character =
            self.recognize_characters(&image_base64, expected_main_rows, language, request)?;
        let CharacterStageResult {
            recognition,
            response: recognition_response,
            combined: combined_stage,
            main_text: main_text_stage,
            ruby: ruby_stage,
            usage: recognition_usage,
            ruby_usage,
            mode: pipeline_mode,
        } = character;
        let main_lines = recognition
            .lines
            .iter()
            .map(|line| {
                language
                    .normalize(&line.text)
                    .map(|(normalized, _)| normalized)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (style, style_response, _) = Self::run_stage(
            &self.validation_client,
            &image_base64,
            "style_recognition",
            &prompt::whole_cue_style(language, &main_lines),
            &style_schema(),
            request,
            validate_whole_cue_style,
        )?;
        let unreadable = recognition.unreadable;
        let color = recognized_color(&style.color)?;
        let (lines, normalizations) =
            assemble_lines(recognition, style.italic, color.as_deref(), language)?;
        let model = recognition_response
            .model
            .clone()
            .unwrap_or_else(|| self.config.recognition.model.clone());
        let raw_response = serde_json::to_string(&json!({
            "pipeline_mode": pipeline_mode,
            "stages": {
                "combined_recognition": combined_stage,
                "main_text_recognition": main_text_stage,
                "ruby_recognition": ruby_stage,
                "style_recognition": parse_json_content(&style_response.content)?,
            },
            "row_estimate": expected_main_rows,
            "usage": {
                "recognition": recognition_usage,
                "ruby": ruby_usage,
                "style": style_response.usage,
            },
            "providers": {
                "recognition": self.config.recognition.redacted(),
                "ruby": self.config.ruby.as_ref().map(ProviderConfig::redacted),
                "validation": self.config.validation.redacted()
            }
        }))?;
        Ok(OcrRecognition {
            document: OcrDocument {
                prompt_version: PROMPT_VERSION.to_owned(),
                provider: self.config.recognition.provider.as_str().to_owned(),
                model,
                language: language.code.to_owned(),
                unreadable,
                lines,
                normalizations,
            },
            raw_response,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }
}

impl OcrBackend for ProviderOcrBackend {
    fn backend_id(&self) -> String {
        let ruby = self.config.ruby.as_ref().map_or_else(
            || "combined".to_owned(),
            |config| {
                format!(
                    "{}:{}:{}",
                    config.provider.as_str(),
                    config.base_url.trim_end_matches('/'),
                    config.model
                )
            },
        );
        format!(
            "{}:{}:{};ruby={ruby};validation={}:{}:{}",
            self.config.recognition.provider.as_str(),
            self.config.recognition.base_url.trim_end_matches('/'),
            self.config.recognition.model,
            self.config.validation.provider.as_str(),
            self.config.validation.base_url.trim_end_matches('/'),
            self.config.validation.model,
        )
    }

    fn recognize(&self, request: &OcrRequest) -> Result<OcrRecognition, OcrError> {
        self.recognize_inner(request)
    }
}

/// Flattens a provider's usage object into the fields that drive cost.
///
/// Providers name these differently and omit whatever does not apply, so every
/// field is optional. `cache_read_input_tokens` is how a prompt-cache hit is
/// confirmed, and `reasoning_tokens` is how `OpenAI` reasoning spend — billed at
/// the output rate — becomes visible.
fn usage_summary(usage: &Value) -> Value {
    let read = |pointers: &[&str]| -> Option<u64> {
        pointers
            .iter()
            .find_map(|pointer| usage.pointer(pointer).and_then(Value::as_u64))
    };
    json!({
        "input_tokens": read(&["/input_tokens", "/prompt_tokens"]),
        "output_tokens": read(&["/output_tokens", "/completion_tokens"]),
        "cache_read_input_tokens": read(&[
            "/cache_read_input_tokens",
            "/prompt_tokens_details/cached_tokens",
        ]),
        "cache_creation_input_tokens": read(&["/cache_creation_input_tokens"]),
        "reasoning_tokens": read(&["/completion_tokens_details/reasoning_tokens"]),
    })
}

#[allow(clippy::too_many_arguments)]
fn emit_stage_event(
    request: &OcrRequest,
    client: &ProviderClient,
    stage: &str,
    attempt: u32,
    level: DiagnosticLevel,
    phase: &str,
    candidate_content: Option<&str>,
    usage: Option<&Value>,
    error: Option<&str>,
) {
    if !rosettacue_diagnostics::enabled() {
        return;
    }
    rosettacue_diagnostics::emit(DiagnosticEvent {
        level,
        source: "ocr",
        category: "pipeline",
        operation: stage,
        phase,
        message: "OCR pipeline stage completed an attempt.",
        duration_ms: None,
        details: json!({
            "cue_id": request.cue_id,
            "cue_index": request.cue_index,
            "attempt": attempt,
            "provider": client.config().provider.as_str(),
            "model": client.config().model,
            "candidate_content": candidate_content,
            "usage": usage.map(usage_summary),
            "error": error
        }),
    });
}

/// Lists models exposed through LM Studio's OpenAI-compatible endpoint.
///
/// # Errors
///
/// Returns an error when the endpoint is invalid, unavailable, or malformed.
pub fn list_lmstudio_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<LmStudioModel>, OcrError> {
    list_provider_models(LlmProvider::LmStudio, base_url, api_key)
}

/// Lists models exposed by any supported provider.
///
/// # Errors
///
/// Returns an error when the endpoint is invalid, unavailable, unauthorized, or malformed.
pub fn list_provider_models(
    provider: LlmProvider,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<LlmModel>, OcrError> {
    Ok(rosettacue_llm::list_models(provider, base_url, api_key)?)
}

#[must_use]
pub fn diagnose_provider(
    provider: LlmProvider,
    base_url: &str,
    api_key: Option<&str>,
) -> ProviderDiagnostic {
    rosettacue_llm::diagnose(provider, base_url, api_key)
}

fn parse_json_content(content: &str) -> Result<Value, OcrError> {
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
        .map_err(|error| OcrError::Validation(format!("model did not return valid JSON: {error}")))
}

fn validate_recognition(
    content: &str,
    expected_main_rows: Option<usize>,
    language: &LanguagePreset,
) -> Result<RecognitionResponse, OcrError> {
    let response = serde_json::from_value::<RecognitionResponse>(parse_json_content(content)?)?;
    let main_lines = validate_main_lines(&response.lines, expected_main_rows, language)?;
    validate_annotations(&response.annotations, &main_lines, language)?;
    Ok(response)
}

fn validate_main_text(
    content: &str,
    expected_main_rows: Option<usize>,
    language: &LanguagePreset,
) -> Result<MainTextResponse, OcrError> {
    let response = serde_json::from_value::<MainTextResponse>(parse_json_content(content)?)?;
    validate_main_lines(&response.lines, expected_main_rows, language)?;
    Ok(response)
}

fn validate_ruby(
    content: &str,
    main_lines: &[String],
    language: &LanguagePreset,
) -> Result<RubyResponse, OcrError> {
    let response = serde_json::from_value::<RubyResponse>(parse_json_content(content)?)?;
    validate_annotations(&response.annotations, main_lines, language)?;
    Ok(response)
}

fn validate_main_lines(
    lines: &[TextValue],
    expected_main_rows: Option<usize>,
    language: &LanguagePreset,
) -> Result<Vec<String>, OcrError> {
    if lines.is_empty() {
        return Err(OcrError::Validation("main text has no lines".to_owned()));
    }
    if let Some(expected) = expected_main_rows
        && lines.len() != expected
    {
        return Err(OcrError::Validation(format!(
            "bitmap analysis found {expected} large main-text rows, but the response returned {}",
            lines.len()
        )));
    }
    let mut main_lines = Vec::with_capacity(lines.len());
    for line in lines {
        let (normalized, _) = language.normalize(&line.text)?;
        if normalized.is_empty() {
            return Err(OcrError::Validation("main text line is empty".to_owned()));
        }
        main_lines.push(normalized);
    }
    Ok(main_lines)
}

fn normalized_main_lines(
    lines: &[TextValue],
    language: &LanguagePreset,
) -> Result<Vec<String>, OcrError> {
    lines
        .iter()
        .map(|line| {
            language
                .normalize(&line.text)
                .map(|(normalized, _)| normalized)
        })
        .collect()
}

fn validate_annotations(
    annotations: &[RawAnnotation],
    main_lines: &[String],
    language: &LanguagePreset,
) -> Result<(), OcrError> {
    let mut ranges_by_line: HashMap<u32, Vec<(usize, usize)>> = HashMap::new();
    for annotation in annotations {
        if annotation.line_index == 0
            || usize::try_from(annotation.line_index).map_or(true, |index| index > main_lines.len())
            || annotation.base_occurrence == 0
            || !matches!(annotation.position.as_str(), "over" | "under")
        {
            return Err(OcrError::Validation(
                "annotation placement is invalid".to_owned(),
            ));
        }
        let annotation_text = language.normalize(&annotation.text)?.0;
        let annotation_base = language.normalize(&annotation.base)?.0;
        if annotation_text.is_empty() || annotation_base.is_empty() {
            return Err(OcrError::Validation(
                "annotation text and base must not be empty".to_owned(),
            ));
        }
        let line = &main_lines[usize::try_from(annotation.line_index - 1)
            .map_err(|_| OcrError::Validation("line index is too large".to_owned()))?];
        let start = find_occurrence(line, &annotation_base, annotation.base_occurrence)
            .ok_or_else(|| {
                OcrError::Validation(format!("annotation base was not found: {annotation_base}"))
            })?;
        ranges_by_line
            .entry(annotation.line_index)
            .or_default()
            .push((start, start + annotation_base.len()));
    }
    for ranges in ranges_by_line.values_mut() {
        ranges.sort_unstable();
        for pair in ranges.windows(2) {
            if pair[1].0 < pair[0].1 && pair[0] != pair[1] {
                return Err(OcrError::Validation("ruby ranges overlap".to_owned()));
            }
        }
    }
    Ok(())
}

fn validate_whole_cue_style(content: &str) -> Result<StyleResponse, OcrError> {
    let response = serde_json::from_value::<StyleResponse>(parse_json_content(content)?)?;
    recognized_color(&response.color)?;
    Ok(response)
}

fn recognized_color(value: &str) -> Result<Option<String>, OcrError> {
    let color = match value {
        "default" => None,
        "black" => Some("#000000"),
        "red" => Some("#FF0000"),
        "orange" => Some("#FF8000"),
        "yellow" => Some("#FFFF00"),
        "green" => Some("#00FF00"),
        "cyan" => Some("#00FFFF"),
        "blue" => Some("#0000FF"),
        "magenta" => Some("#FF00FF"),
        other => {
            return Err(OcrError::Validation(format!(
                "unsupported recognized text color: {other}"
            )));
        }
    };
    Ok(color.map(str::to_owned))
}

fn assemble_lines(
    recognition: RecognitionResponse,
    italic: bool,
    color: Option<&str>,
    language: &LanguagePreset,
) -> Result<(Vec<OcrLine>, Vec<NormalizationRecord>), OcrError> {
    let mut records = Vec::new();
    let mut by_line: HashMap<u32, Vec<ValidatedAnnotation>> = HashMap::new();
    for (annotation_index, annotation) in recognition.annotations.into_iter().enumerate() {
        let (text, text_events) = language.normalize(&annotation.text)?;
        let (base, base_events) = language.normalize(&annotation.base)?;
        add_records(
            &mut records,
            text_events,
            "annotation_text",
            annotation.line_index,
            Some(annotation_index + 1),
        );
        add_records(
            &mut records,
            base_events,
            "annotation_base",
            annotation.line_index,
            Some(annotation_index + 1),
        );
        by_line
            .entry(annotation.line_index)
            .or_default()
            .push(ValidatedAnnotation {
                line_index: annotation.line_index,
                text,
                base,
                base_occurrence: annotation.base_occurrence,
                position: if annotation.position == "over" {
                    RubyPosition::Over
                } else {
                    RubyPosition::Under
                },
            });
    }
    let mut lines = Vec::with_capacity(recognition.lines.len());
    for (line_offset, raw_line) in recognition.lines.into_iter().enumerate() {
        let line_index = u32::try_from(line_offset + 1)
            .map_err(|_| OcrError::Validation("too many OCR lines".to_owned()))?;
        let (text, events) = language.normalize(&raw_line.text)?;
        add_records(&mut records, events, "text", line_index, None);
        let spans = assemble_spans(
            &text,
            by_line.remove(&line_index).unwrap_or_default(),
            italic,
            color,
        )?;
        lines.push(OcrLine { text, spans });
    }
    Ok((lines, records))
}

fn assemble_spans(
    text: &str,
    annotations: Vec<ValidatedAnnotation>,
    italic: bool,
    color: Option<&str>,
) -> Result<Vec<OcrSpan>, OcrError> {
    let styles = if italic {
        vec![TextStyle::Italic]
    } else {
        Vec::new()
    };
    let mut ranges: BTreeMap<(usize, usize), Vec<RubyAnnotation>> = BTreeMap::new();
    for annotation in annotations {
        let start = find_occurrence(text, &annotation.base, annotation.base_occurrence)
            .ok_or_else(|| {
                OcrError::Validation(format!(
                    "ruby base was not found on line {}",
                    annotation.line_index
                ))
            })?;
        ranges
            .entry((start, start + annotation.base.len()))
            .or_default()
            .push(RubyAnnotation {
                text: annotation.text,
                position: annotation.position,
            });
    }
    let mut spans = Vec::new();
    let mut cursor = 0;
    for ((start, end), annotations) in ranges {
        if start < cursor {
            return Err(OcrError::Validation("ruby ranges overlap".to_owned()));
        }
        if start > cursor {
            spans.push(OcrSpan::Text {
                text: text[cursor..start].to_owned(),
                styles: styles.clone(),
                color: color.map(str::to_owned),
            });
        }
        spans.push(OcrSpan::Ruby {
            base: text[start..end].to_owned(),
            annotations,
            styles: styles.clone(),
            color: color.map(str::to_owned),
        });
        cursor = end;
    }
    if cursor < text.len() {
        spans.push(OcrSpan::Text {
            text: text[cursor..].to_owned(),
            styles: styles.clone(),
            color: color.map(str::to_owned),
        });
    }
    if spans.is_empty() {
        spans.push(OcrSpan::Text {
            text: text.to_owned(),
            styles,
            color: color.map(str::to_owned),
        });
    }
    Ok(spans)
}

fn find_occurrence(text: &str, needle: &str, occurrence: u32) -> Option<usize> {
    if needle.is_empty() || occurrence == 0 {
        return None;
    }
    text.match_indices(needle)
        .nth(usize::try_from(occurrence - 1).ok()?)
        .map(|(index, _)| index)
}

fn add_records(
    records: &mut Vec<NormalizationRecord>,
    events: Vec<NormalizationEvent>,
    field: &str,
    line_index: u32,
    annotation_index: Option<usize>,
) {
    records.extend(events.into_iter().map(|event| NormalizationRecord {
        rule: event.rule.to_owned(),
        field: field.to_owned(),
        line_index,
        annotation_index: annotation_index.and_then(|index| u32::try_from(index).ok()),
        before: event.before,
        after: event.after,
    }));
}

fn combined_recognition_schema() -> Value {
    json!({ "type": "json_schema", "json_schema": { "name": "subtitle_character_recognition", "strict": true, "schema": { "type": "object", "properties": { "lines": { "type": "array", "minItems": 1, "items": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"], "additionalProperties": false } }, "annotations": { "type": "array", "items": { "type": "object", "properties": { "line_index": { "type": "integer", "minimum": 1 }, "text": { "type": "string" }, "base": { "type": "string" }, "base_occurrence": { "type": "integer", "minimum": 1 }, "position": { "enum": ["over", "under"] } }, "required": ["line_index", "text", "base", "base_occurrence", "position"], "additionalProperties": false } }, "unreadable": { "type": "boolean" } }, "required": ["lines", "annotations", "unreadable"], "additionalProperties": false } } })
}

fn main_text_schema() -> Value {
    json!({ "type": "json_schema", "json_schema": { "name": "subtitle_main_text_recognition", "strict": true, "schema": { "type": "object", "properties": { "lines": { "type": "array", "minItems": 1, "items": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"], "additionalProperties": false } }, "unreadable": { "type": "boolean" } }, "required": ["lines", "unreadable"], "additionalProperties": false } } })
}

fn ruby_schema() -> Value {
    json!({ "type": "json_schema", "json_schema": { "name": "subtitle_ruby_recognition", "strict": true, "schema": { "type": "object", "properties": { "annotations": { "type": "array", "items": { "type": "object", "properties": { "line_index": { "type": "integer", "minimum": 1 }, "text": { "type": "string" }, "base": { "type": "string" }, "base_occurrence": { "type": "integer", "minimum": 1 }, "position": { "enum": ["over", "under"] } }, "required": ["line_index", "text", "base", "base_occurrence", "position"], "additionalProperties": false } }, "unreadable": { "type": "boolean" } }, "required": ["annotations", "unreadable"], "additionalProperties": false } } })
}

fn style_schema() -> Value {
    json!({ "type": "json_schema", "json_schema": { "name": "subtitle_whole_cue_style", "strict": true, "schema": { "type": "object", "properties": { "italic": { "type": "boolean" }, "color": { "enum": ["default", "black", "red", "orange", "yellow", "green", "cyan", "blue", "magenta"] } }, "required": ["italic", "color"], "additionalProperties": false } } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_pipeline_config_ignores_debug_logging() {
        let provider = ProviderConfig::default();
        let config: OcrPipelineConfig = serde_json::from_value(json!({
            "recognition": provider.clone(),
            "validation": provider,
            "debug_logging": true,
        }))
        .expect("legacy OCR pipeline config");

        assert!(config.recognition.model.is_empty());
        assert!(config.ruby.is_none());
    }

    #[test]
    fn redacts_the_optional_ruby_provider() {
        let provider = ProviderConfig {
            api_key: Some("session-secret".to_owned()),
            ..ProviderConfig::default()
        };
        let config = OcrPipelineConfig {
            recognition: provider.clone(),
            ruby: Some(provider.clone()),
            validation: provider,
        };

        let redacted = config.redacted();

        assert!(redacted.recognition.api_key.is_none());
        assert!(redacted.ruby.expect("ruby profile").api_key.is_none());
        assert!(redacted.validation.api_key.is_none());
    }

    #[test]
    fn validates_and_assembles_ruby() {
        let language = languages::resolve("jpn").expect("Japanese preset");
        let recognition = validate_recognition(
            r#"{"lines":[{"text":"司る"}],"annotations":[{"line_index":1,"text":"つかさど","base":"司","base_occurrence":1,"position":"over"}],"unreadable":false}"#,
            Some(1),
            language,
        )
        .expect("recognition response");
        let (lines, _) =
            assemble_lines(recognition, false, None, language).expect("assemble lines");
        assert_eq!(lines[0].text, "司る");
        assert!(matches!(lines[0].spans[0], OcrSpan::Ruby { .. }));
    }

    #[test]
    fn validates_split_ruby_against_normalized_main_text() {
        let language = languages::resolve("jpn").expect("Japanese preset");
        let main = validate_main_text(
            r#"{"lines":[{"text":"(司る)"}],"unreadable":false}"#,
            Some(1),
            language,
        )
        .expect("main text response");
        let normalized = normalized_main_lines(&main.lines, language).expect("normalized lines");
        assert_eq!(normalized, ["（司る）"]);
        let ruby = validate_ruby(
            r#"{"annotations":[{"line_index":1,"text":"つかさど","base":"司","base_occurrence":1,"position":"over"}],"unreadable":false}"#,
            &normalized,
            language,
        )
        .expect("ruby response");
        let recognition = RecognitionResponse {
            lines: main.lines,
            annotations: ruby.annotations,
            unreadable: main.unreadable || ruby.unreadable,
        };

        let (lines, records) =
            assemble_lines(recognition, false, None, language).expect("assembled lines");

        assert_eq!(lines[0].text, "（司る）");
        assert!(matches!(lines[0].spans[1], OcrSpan::Ruby { .. }));
        assert!(
            records
                .iter()
                .any(|record| record.rule == "japanese-fullwidth-symbol-v1")
        );
    }

    #[test]
    fn rejects_overlapping_ruby_ranges_during_stage_validation() {
        let language = languages::resolve("jpn").expect("Japanese preset");
        let error = validate_ruby(
            r#"{"annotations":[{"line_index":1,"text":"かんじ","base":"漢字","base_occurrence":1,"position":"over"},{"line_index":1,"text":"じ","base":"字字幕","base_occurrence":1,"position":"over"}],"unreadable":false}"#,
            &["漢字字幕".to_owned()],
            language,
        )
        .expect_err("overlapping ruby must fail");

        assert!(error.to_string().contains("overlap"));
    }

    #[test]
    fn rejects_a_main_response_that_omits_a_detected_row() {
        let language = languages::resolve("eng").expect("English preset");
        let error = validate_recognition(
            r#"{"lines":[{"text":"first"}],"annotations":[],"unreadable":false}"#,
            Some(2),
            language,
        )
        .expect_err("missing row must fail");
        assert!(error.to_string().contains("found 2"));
    }

    #[test]
    fn applies_whole_cue_italic_to_text_and_ruby_spans() {
        let language = languages::resolve("jpn").expect("Japanese preset");
        let recognition = validate_recognition(
            r#"{"lines":[{"text":"Uは 司る人"}],"annotations":[{"line_index":1,"text":"つかさど","base":"司","base_occurrence":1,"position":"over"}],"unreadable":false}"#,
            Some(1),
            language,
        )
        .expect("recognition response");
        let style = validate_whole_cue_style(r#"{"italic":true,"color":"default"}"#)
            .expect("whole cue style");
        let (lines, _) =
            assemble_lines(recognition, style.italic, None, language).expect("assembled line");
        assert!(matches!(
            &lines[0].spans[0],
            OcrSpan::Text { styles, .. } if styles == &[TextStyle::Italic]
        ));
        assert!(matches!(
            &lines[0].spans[1],
            OcrSpan::Ruby { styles, .. } if styles == &[TextStyle::Italic]
        ));
        assert!(matches!(
            &lines[0].spans[2],
            OcrSpan::Text { styles, .. } if styles == &[TextStyle::Italic]
        ));
    }

    #[test]
    fn stores_a_clear_non_white_color_on_every_span() {
        let language = languages::resolve("eng").expect("English preset");
        let recognition = validate_recognition(
            r#"{"lines":[{"text":"Alert"}],"annotations":[],"unreadable":false}"#,
            Some(1),
            language,
        )
        .expect("recognition response");
        let style =
            validate_whole_cue_style(r#"{"italic":false,"color":"red"}"#).expect("whole cue style");
        let color = recognized_color(&style.color).expect("known color");
        let (lines, _) = assemble_lines(recognition, style.italic, color.as_deref(), language)
            .expect("assembled line");

        assert_eq!(lines[0].spans[0].color(), Some("#FF0000"));
    }

    #[test]
    fn usage_summary_reads_the_anthropic_field_names() {
        let summary = usage_summary(&json!({
            "input_tokens": 1200,
            "output_tokens": 150,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 1100
        }));

        assert_eq!(summary["input_tokens"], 1200);
        assert_eq!(summary["output_tokens"], 150);
        assert_eq!(summary["cache_read_input_tokens"], 1100);
        assert_eq!(summary["cache_creation_input_tokens"], 0);
        assert_eq!(summary["reasoning_tokens"], Value::Null);
    }

    #[test]
    fn usage_summary_reads_the_openai_field_names() {
        let summary = usage_summary(&json!({
            "prompt_tokens": 1380,
            "completion_tokens": 190,
            "total_tokens": 1570,
            "prompt_tokens_details": { "cached_tokens": 1024 },
            "completion_tokens_details": { "reasoning_tokens": 0 }
        }));

        assert_eq!(summary["input_tokens"], 1380);
        assert_eq!(summary["output_tokens"], 190);
        assert_eq!(summary["cache_read_input_tokens"], 1024);
        assert_eq!(summary["reasoning_tokens"], 0);
        assert_eq!(summary["cache_creation_input_tokens"], Value::Null);
    }

    #[test]
    fn usage_summary_tolerates_a_provider_that_reports_nothing() {
        let summary = usage_summary(&Value::Null);

        for field in [
            "input_tokens",
            "output_tokens",
            "cache_read_input_tokens",
            "cache_creation_input_tokens",
            "reasoning_tokens",
        ] {
            assert_eq!(summary[field], Value::Null, "{field} must stay absent");
        }
    }
}
