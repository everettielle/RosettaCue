use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
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
use crate::prompt::{self, PROMPT_VERSION, SYSTEM_PROMPT};
use crate::row_detection::estimate_main_rows;
use crate::{OcrBackend, OcrError, OcrRecognition, OcrRequest};

pub type LmStudioConfig = ProviderConfig;
pub type LmStudioModel = LlmModel;
pub type LmStudioBackend = ProviderOcrBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrPipelineConfig {
    pub recognition: ProviderConfig,
    pub validation: ProviderConfig,
}

impl OcrPipelineConfig {
    #[must_use]
    pub fn single(config: ProviderConfig) -> Self {
        Self {
            recognition: config.clone(),
            validation: config,
        }
    }

    #[must_use]
    pub fn redacted(&self) -> Self {
        Self {
            recognition: self.recognition.redacted(),
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
    validation_client: ProviderClient,
}

#[derive(Debug, Deserialize)]
struct MainResponse {
    lines: Vec<TextValue>,
    unreadable: bool,
}

#[derive(Debug, Deserialize)]
struct TextValue {
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnnotationResponse {
    annotations: Vec<RawAnnotation>,
    unreadable: bool,
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

    /// Creates an OCR backend with independent recognition and validation providers.
    ///
    /// # Errors
    ///
    /// Returns an error when either provider configuration is invalid.
    pub fn with_pipeline(config: OcrPipelineConfig) -> Result<Self, OcrError> {
        let recognition_client = ProviderClient::new(config.recognition.clone())?;
        let validation_client = ProviderClient::new(config.validation.clone())?;
        Ok(Self {
            config,
            recognition_client,
            validation_client,
        })
    }

    fn run_stage<T>(
        client: &ProviderClient,
        image_url: &str,
        stage: &str,
        user_text: &str,
        schema: &Value,
        validate: impl Fn(&str) -> Result<T, OcrError>,
    ) -> Result<(T, CompletionResponse, u32), OcrError> {
        let mut previous: Option<String> = None;
        let mut validation_error: Option<String> = None;
        for attempt in 1..=client.config().max_attempts {
            let correction = validation_error
                .as_deref()
                .map(|error| prompt::retry(stage, error));
            let response = client.complete(&CompletionRequest {
                system: SYSTEM_PROMPT,
                user_text,
                image_png_base64: Some(image_url),
                response_format: Some(schema),
                previous_response: previous.as_deref(),
                correction: correction.as_deref(),
            })?;
            match validate(&response.content) {
                Ok(value) => return Ok((value, response, attempt)),
                Err(error) if attempt < client.config().max_attempts => {
                    validation_error = Some(error.to_string());
                    previous = Some(response.content);
                }
                Err(error) => return Err(error),
            }
        }
        Err(OcrError::Validation(format!(
            "{stage} exhausted all attempts"
        )))
    }
}

impl OcrBackend for ProviderOcrBackend {
    fn backend_id(&self) -> String {
        format!(
            "{}:{}:{};validation={}:{}:{}",
            self.config.recognition.provider.as_str(),
            self.config.recognition.base_url.trim_end_matches('/'),
            self.config.recognition.model,
            self.config.validation.provider.as_str(),
            self.config.validation.base_url.trim_end_matches('/'),
            self.config.validation.model,
        )
    }

    fn recognize(&self, request: &OcrRequest) -> Result<OcrRecognition, OcrError> {
        let language = languages::resolve(&request.language)?;
        let started = Instant::now();
        let image = std::fs::read(&request.image_path)?;
        let expected_main_rows = estimate_main_rows(&image);
        let image_base64 = BASE64.encode(image);
        let (main, main_response, _) = Self::run_stage(
            &self.recognition_client,
            &image_base64,
            "main-text",
            &prompt::main_text(language, expected_main_rows),
            &main_schema(),
            |content| validate_main(content, expected_main_rows, language),
        )?;
        let main_lines = main
            .lines
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>();
        let (annotations, annotation_response, _) = Self::run_stage(
            &self.validation_client,
            &image_base64,
            "annotation",
            &prompt::annotations(language, &main_lines),
            &annotation_schema(),
            |content| validate_annotations(content, &main_lines, language),
        )?;
        let (style, style_response, _) = Self::run_stage(
            &self.validation_client,
            &image_base64,
            "style",
            &prompt::whole_cue_style(language, &main_lines),
            &style_schema(),
            validate_whole_cue_style,
        )?;
        let unreadable = main.unreadable || annotations.unreadable;
        let (lines, normalizations) = assemble_lines(main, annotations, style.italic, language)?;
        let model = style_response
            .model
            .clone()
            .or_else(|| annotation_response.model.clone())
            .or_else(|| main_response.model.clone())
            .unwrap_or_else(|| self.config.recognition.model.clone());
        let raw_response = serde_json::to_string(&json!({
            "main_text": parse_json_content(&main_response.content)?,
            "annotations": parse_json_content(&annotation_response.content)?,
            "style": parse_json_content(&style_response.content)?,
            "row_estimate": expected_main_rows,
            "usage": [main_response.usage, annotation_response.usage, style_response.usage],
            "providers": {
                "recognition": self.config.recognition.redacted(),
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
        .map_err(|_| OcrError::Validation("model did not return valid JSON".to_owned()))
}

fn validate_main(
    content: &str,
    expected_main_rows: Option<usize>,
    language: &LanguagePreset,
) -> Result<MainResponse, OcrError> {
    let mut response = serde_json::from_value::<MainResponse>(parse_json_content(content)?)?;
    if response.lines.is_empty() {
        return Err(OcrError::Validation("main text has no lines".to_owned()));
    }
    if let Some(expected) = expected_main_rows
        && response.lines.len() != expected
    {
        return Err(OcrError::Validation(format!(
            "bitmap analysis found {expected} large main-text rows, but the response returned {}",
            response.lines.len()
        )));
    }
    for line in &mut response.lines {
        let (normalized, _) = language.normalize(&line.text)?;
        if normalized.is_empty() {
            return Err(OcrError::Validation("main text line is empty".to_owned()));
        }
        line.text = normalized;
    }
    Ok(response)
}

fn validate_annotations(
    content: &str,
    main_lines: &[String],
    language: &LanguagePreset,
) -> Result<AnnotationResponse, OcrError> {
    let mut response = serde_json::from_value::<AnnotationResponse>(parse_json_content(content)?)?;
    for annotation in &mut response.annotations {
        if annotation.line_index == 0
            || usize::try_from(annotation.line_index).map_or(true, |index| index > main_lines.len())
            || annotation.base_occurrence == 0
            || !matches!(annotation.position.as_str(), "over" | "under")
        {
            return Err(OcrError::Validation(
                "annotation placement is invalid".to_owned(),
            ));
        }
        annotation.text = language.normalize(&annotation.text)?.0;
        annotation.base = language.normalize(&annotation.base)?.0;
        let line = &main_lines[usize::try_from(annotation.line_index - 1)
            .map_err(|_| OcrError::Validation("line index is too large".to_owned()))?];
        if find_occurrence(line, &annotation.base, annotation.base_occurrence).is_none() {
            return Err(OcrError::Validation(format!(
                "annotation base was not found: {}",
                annotation.base
            )));
        }
    }
    Ok(response)
}

fn validate_whole_cue_style(content: &str) -> Result<StyleResponse, OcrError> {
    Ok(serde_json::from_value(parse_json_content(content)?)?)
}

fn assemble_lines(
    main: MainResponse,
    annotations: AnnotationResponse,
    italic: bool,
    language: &LanguagePreset,
) -> Result<(Vec<OcrLine>, Vec<NormalizationRecord>), OcrError> {
    let mut records = Vec::new();
    let mut by_line: HashMap<u32, Vec<ValidatedAnnotation>> = HashMap::new();
    for (annotation_index, annotation) in annotations.annotations.into_iter().enumerate() {
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
    let mut lines = Vec::with_capacity(main.lines.len());
    for (line_offset, raw_line) in main.lines.into_iter().enumerate() {
        let line_index = u32::try_from(line_offset + 1)
            .map_err(|_| OcrError::Validation("too many OCR lines".to_owned()))?;
        let (text, events) = language.normalize(&raw_line.text)?;
        add_records(&mut records, events, "text", line_index, None);
        let spans = assemble_spans(
            &text,
            by_line.remove(&line_index).unwrap_or_default(),
            italic,
        )?;
        lines.push(OcrLine { text, spans });
    }
    Ok((lines, records))
}

fn assemble_spans(
    text: &str,
    annotations: Vec<ValidatedAnnotation>,
    italic: bool,
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
            });
        }
        spans.push(OcrSpan::Ruby {
            base: text[start..end].to_owned(),
            annotations,
            styles: styles.clone(),
        });
        cursor = end;
    }
    if cursor < text.len() {
        spans.push(OcrSpan::Text {
            text: text[cursor..].to_owned(),
            styles: styles.clone(),
        });
    }
    if spans.is_empty() {
        spans.push(OcrSpan::Text {
            text: text.to_owned(),
            styles,
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

fn main_schema() -> Value {
    json!({ "type": "json_schema", "json_schema": { "name": "subtitle_main_text", "strict": true, "schema": { "type": "object", "properties": { "lines": { "type": "array", "minItems": 1, "items": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"], "additionalProperties": false } }, "unreadable": { "type": "boolean" } }, "required": ["lines", "unreadable"], "additionalProperties": false } } })
}

fn annotation_schema() -> Value {
    json!({ "type": "json_schema", "json_schema": { "name": "subtitle_annotations", "strict": true, "schema": { "type": "object", "properties": { "annotations": { "type": "array", "items": { "type": "object", "properties": { "line_index": { "type": "integer", "minimum": 1 }, "text": { "type": "string" }, "base": { "type": "string" }, "base_occurrence": { "type": "integer", "minimum": 1 }, "position": { "enum": ["over", "under"] } }, "required": ["line_index", "text", "base", "base_occurrence", "position"], "additionalProperties": false } }, "unreadable": { "type": "boolean" } }, "required": ["annotations", "unreadable"], "additionalProperties": false } } })
}

fn style_schema() -> Value {
    json!({ "type": "json_schema", "json_schema": { "name": "subtitle_whole_cue_style", "strict": true, "schema": { "type": "object", "properties": { "italic": { "type": "boolean" } }, "required": ["italic"], "additionalProperties": false } } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_assembles_ruby() {
        let language = languages::resolve("jpn").expect("Japanese preset");
        let main = validate_main(
            r#"{"lines":[{"text":"司る"}],"unreadable":false}"#,
            Some(1),
            language,
        )
        .expect("main response");
        let annotations = validate_annotations(
            r#"{"annotations":[{"line_index":1,"text":"つかさど","base":"司","base_occurrence":1,"position":"over"}],"unreadable":false}"#,
            &["司る".to_owned()],
            language,
        )
        .expect("annotation response");
        let (lines, _) =
            assemble_lines(main, annotations, false, language).expect("assemble lines");
        assert_eq!(lines[0].text, "司る");
        assert!(matches!(lines[0].spans[0], OcrSpan::Ruby { .. }));
    }

    #[test]
    fn rejects_a_main_response_that_omits_a_detected_row() {
        let language = languages::resolve("eng").expect("English preset");
        let error = validate_main(
            r#"{"lines":[{"text":"first"}],"unreadable":false}"#,
            Some(2),
            language,
        )
        .expect_err("missing row must fail");
        assert!(error.to_string().contains("found 2"));
    }

    #[test]
    fn applies_whole_cue_italic_to_text_and_ruby_spans() {
        let language = languages::resolve("jpn").expect("Japanese preset");
        let main = validate_main(
            r#"{"lines":[{"text":"Uは 司る人"}],"unreadable":false}"#,
            Some(1),
            language,
        )
        .expect("main response");
        let annotations = validate_annotations(
            r#"{"annotations":[{"line_index":1,"text":"つかさど","base":"司","base_occurrence":1,"position":"over"}],"unreadable":false}"#,
            &["Uは 司る人".to_owned()],
            language,
        )
        .expect("annotations");
        let style = validate_whole_cue_style(r#"{"italic":true}"#).expect("whole cue style");
        let (lines, _) =
            assemble_lines(main, annotations, style.italic, language).expect("assembled line");
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
}
