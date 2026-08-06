use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::OcrError;

static CONTROL_ELLIPSIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\\u{0013}{2,}").expect("valid control ellipsis regex"));
static DOT_ELLIPSIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\.|．){3,}|…+").expect("valid centered ellipsis regex"));
static CONTROL_LONG_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\\u{0014}+").expect("valid long mark regex"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizationEvent {
    pub rule: &'static str,
    pub before: String,
    pub after: String,
}

pub fn normalize_japanese(text: &str) -> Result<(String, Vec<NormalizationEvent>), OcrError> {
    let mut current = text.to_owned();
    let mut events = Vec::new();
    apply(&mut current, &mut events, "unicode-nfc", |value| {
        value.nfc().collect()
    });
    apply(
        &mut current,
        &mut events,
        "japanese-control-ellipsis-v1",
        |value| CONTROL_ELLIPSIS.replace_all(value, "⋯").into_owned(),
    );
    apply(
        &mut current,
        &mut events,
        "japanese-centered-ellipsis-v1",
        |value| DOT_ELLIPSIS.replace_all(value, "⋯").into_owned(),
    );
    apply(
        &mut current,
        &mut events,
        "japanese-prolonged-sound-mark-v1",
        |value| {
            CONTROL_LONG_MARK
                .replace_all(value, "ー")
                .chars()
                .map(|character| match character {
                    '—' | '―' | 'ｰ' => 'ー',
                    other => other,
                })
                .collect()
        },
    );
    apply(
        &mut current,
        &mut events,
        "japanese-fullwidth-symbol-v1",
        fullwidth_symbols,
    );
    if let Some(control) = current.chars().find(|character| character.is_control()) {
        return Err(OcrError::Validation(format!(
            "OCR text contains control character U+{:04X}",
            u32::from(control)
        )));
    }
    Ok((current, events))
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

fn fullwidth_symbols(value: &str) -> String {
    const ASCII_SYMBOLS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
    value
        .chars()
        .map(|character| {
            if ASCII_SYMBOLS.contains(character) {
                char::from_u32(u32::from(character) + 0xfee0).unwrap_or(character)
            } else {
                match character {
                    '｡' => '。',
                    '､' => '、',
                    '｢' => '「',
                    '｣' => '」',
                    '･' => '・',
                    other => other,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_japanese_symbols() {
        let (normalized, events) =
            normalize_japanese("(テスト)...—!").expect("normalize Japanese symbols");
        assert_eq!(normalized, "（テスト）⋯ー！");
        assert!(!events.is_empty());
    }

    #[test]
    fn accepts_model_specific_control_sequences() {
        let (normalized, _) =
            normalize_japanese("あ\u{13}\u{13}い\u{14}").expect("normalize controls");
        assert_eq!(normalized, "あ⋯いー");
    }
}
