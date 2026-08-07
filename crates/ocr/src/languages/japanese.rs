use std::sync::LazyLock;

use regex::Regex;

use super::{BlockOrder, LanguagePreset, NormalizationEvent, apply, normalize_nfc};

static DOT_ELLIPSIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\.|．){3,}|…+").expect("valid centered ellipsis regex"));

const MAIN_TEXT_INSTRUCTION: &str = r"Use Japanese full-width punctuation and symbols. Convert ASCII or halfwidth symbols such as ()!? punctuation, quotes, and brackets to their full-width forms, but keep Latin letters and digits unchanged. Use the full-width Japanese centered ellipsis ⋯ (U+22EF, LaTeX \cdots), never … or three ASCII/fullwidth periods. Use the Japanese prolonged sound mark ー (U+30FC) for an ambiguous long horizontal sound mark; normalize —, ―, and halfwidth ｰ to ー.";

const ANNOTATION_INSTRUCTION: &str = r"Choose each base from visual horizontal alignment in the bitmap. Use the smallest exact contiguous substring directly above or below the annotation: prefer the main glyph under its horizontal center, and add neighboring glyphs only when the annotation visibly spans them. For kana readings in mixed kanji-kana text, choose the aligned lexical kanji or other non-kana glyphs; exclude particles, okurigana, and inflectional kana outside the annotation span. Never use punctuation, brackets, spacing or sound-length marks, ellipses, or other standalone symbols as the sole base. Use a kana-only base only when the annotation is clearly aligned with lexical kana and no kanji in the same visual word is a plausible aligned base. Use pronunciation only to reject an implausible visual candidate, never to expand the base. Apply the same minimum-range rule to Latin letters and digits.";

pub(super) const PRESET: LanguagePreset = LanguagePreset::new(
    "jpn",
    "Japanese",
    MAIN_TEXT_INSTRUCTION,
    ANNOTATION_INSTRUCTION,
    BlockOrder::RightToLeft,
    normalize_japanese,
    false,
);

fn normalize_japanese(text: &str) -> (String, Vec<NormalizationEvent>) {
    let (mut current, mut events) = normalize_nfc(text);
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
            value
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
    (current, events)
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
        let (normalized, events) = normalize_japanese("(テスト)...—!");
        assert_eq!(normalized, "（テスト）⋯ー！");
        assert!(!events.is_empty());
    }
}
