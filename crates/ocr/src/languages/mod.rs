mod english;
mod french;
mod german;
mod italian;
mod japanese;
mod korean;
mod spanish;

pub(crate) use rosettacue_layout::BlockOrder;
use unicode_normalization::UnicodeNormalization;

use crate::OcrError;

type Normalizer = fn(&str) -> (String, Vec<NormalizationEvent>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct LanguagePreset {
    pub code: &'static str,
    pub display_name: &'static str,
    pub main_text_instruction: &'static str,
    pub annotation_instruction: &'static str,
    /// How blocks sitting side by side are read. Vertical Japanese goes right
    /// to left; every other supported script goes left to right.
    pub block_order: BlockOrder,
    normalize: Normalizer,
    reject_control_characters: bool,
}

impl LanguagePreset {
    pub(crate) const fn new(
        code: &'static str,
        display_name: &'static str,
        main_text_instruction: &'static str,
        annotation_instruction: &'static str,
        block_order: BlockOrder,
        normalize: Normalizer,
        reject_control_characters: bool,
    ) -> Self {
        Self {
            code,
            display_name,
            main_text_instruction,
            annotation_instruction,
            block_order,
            normalize,
            reject_control_characters,
        }
    }

    pub(crate) fn normalize(
        &self,
        text: &str,
    ) -> Result<(String, Vec<NormalizationEvent>), OcrError> {
        let normalized = (self.normalize)(text);
        if self.reject_control_characters {
            reject_controls(&normalized.0)?;
        }
        Ok(normalized)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizationEvent {
    pub rule: &'static str,
    pub before: String,
    pub after: String,
}

pub(crate) fn resolve(language: &str) -> Result<&'static LanguagePreset, OcrError> {
    let normalized = language.trim().to_ascii_lowercase();
    let preset = match normalized.as_str() {
        "en" | "eng" | "english" => &english::PRESET,
        "fr" | "fra" | "fre" | "french" => &french::PRESET,
        "de" | "deu" | "ger" | "german" => &german::PRESET,
        "it" | "ita" | "italian" => &italian::PRESET,
        "ja" | "jpn" | "japanese" => &japanese::PRESET,
        "ko" | "kor" | "korean" => &korean::PRESET,
        "es" | "spa" | "spanish" => &spanish::PRESET,
        _ => {
            return Err(OcrError::InvalidConfig(format!(
                "language profile is not supported: {language}"
            )));
        }
    };
    Ok(preset)
}

fn normalize_text(text: &str) -> (String, Vec<NormalizationEvent>) {
    normalize_nfc(text)
}

fn normalize_nfc(text: &str) -> (String, Vec<NormalizationEvent>) {
    let mut current = text.to_owned();
    let mut events = Vec::new();
    apply(&mut current, &mut events, "unicode-nfc", |value| {
        value.nfc().collect()
    });
    (current, events)
}

fn apply(
    current: &mut String,
    events: &mut Vec<NormalizationEvent>,
    rule: &'static str,
    transform: impl FnOnce(&str) -> String,
) {
    let normalized = transform(current);
    if normalized != *current {
        events.push(NormalizationEvent {
            rule,
            before: current.clone(),
            after: normalized.clone(),
        });
        *current = normalized;
    }
}

fn reject_controls(text: &str) -> Result<(), OcrError> {
    if let Some(control) = text.chars().find(|character| character.is_control()) {
        return Err(OcrError::Validation(format!(
            "OCR text contains control character U+{:04X}",
            u32::from(control)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_supported_language_codes_names_and_aliases() {
        let cases = [
            ("English", "eng"),
            ("fre", "fra"),
            ("ger", "deu"),
            ("Italian", "ita"),
            ("jpn", "jpn"),
            ("Japanese", "jpn"),
            ("ko", "kor"),
            ("es", "spa"),
        ];

        for (input, expected) in cases {
            assert_eq!(resolve(input).expect("supported language").code, expected);
        }
    }

    #[test]
    fn rejects_unsupported_languages() {
        let error = resolve("zho").expect_err("unsupported language must fail");
        assert!(error.to_string().contains("zho"));
    }

    #[test]
    fn latin_presets_share_literal_recognition_guidance() {
        let english = resolve("eng").expect("English preset");
        for language in ["fra", "spa", "deu", "ita"] {
            let preset = resolve(language).expect("Latin preset");
            assert_eq!(preset.main_text_instruction, english.main_text_instruction);
            assert_eq!(
                preset.annotation_instruction,
                english.annotation_instruction
            );
        }
    }

    #[test]
    fn common_normalization_preserves_language_specific_letters() {
        for (language, text) in [
            ("eng", "naïve"),
            ("fra", "cœur"),
            ("spa", "¿Qué?"),
            ("deu", "Straße"),
            ("ita", "perché"),
        ] {
            let normalized = resolve(language)
                .expect("language preset")
                .normalize(text)
                .expect("valid text")
                .0;
            assert_eq!(normalized, text);
        }
    }
}
