use serde_json::json;

use crate::languages::LanguagePreset;

pub const PROMPT_VERSION: &str = "subtitle-ocr-v5";

pub const SYSTEM_PROMPT: &str = r"You are a literal OCR engine for Blu-ray subtitle images.
Transcribe only characters that are visibly present. Never translate, paraphrase,
correct grammar, complete a sentence, or add an explanation. Preserve the main
subtitle's punctuation and line breaks. If a main-text character cannot be read,
write the replacement character � instead of guessing. Never include C0 or C1
control characters in the response.

Treat large main text and small aligned annotations as different layers. Small
text immediately above or below a main-text range is ruby annotation, not an
additional main subtitle line.";

fn row_hint(expected_main_rows: Option<usize>) -> String {
    expected_main_rows.map_or_else(String::new, |count| {
        format!(
            "\nDeterministic bitmap analysis found exactly {count} likely large main-text rows. Return exactly {count} lines items. Small ruby rows are not included in this count."
        )
    })
}

pub fn combined_recognition(preset: &LanguagePreset, expected_main_rows: Option<usize>) -> String {
    format!(
        "The expected subtitle language is {} ({}).\nThis is combined character and ruby recognition. In one response, transcribe every large main subtitle row and every small ruby or furigana annotation aligned above or below it. Create exactly one lines item for every main-text row, ordered top to bottom. Carefully scan the full image before answering and transcribe each complete row left to right, including punctuation. Do not include small annotation text in lines. For each annotation, line_index is the 1-based target main line, base is the exact contiguous substring it annotates, base_occurrence identifies which occurrence of that substring is annotated, text is the small visible annotation, and position is over or under. Return an empty annotations array when none exist. Set unreadable=true when any visible main or annotation glyph was replaced by �.{}\nMandatory language-specific main-text guidance: {}\nMandatory language-specific annotation guidance: {}",
        preset.display_name,
        preset.code,
        row_hint(expected_main_rows),
        preset.main_text_instruction,
        preset.annotation_instruction,
    )
}

pub fn main_text_recognition(preset: &LanguagePreset, expected_main_rows: Option<usize>) -> String {
    format!(
        "The expected subtitle language is {} ({}).\nThis is main-text character recognition. Transcribe only the large main subtitle rows, ordered top to bottom. Carefully scan the full image and transcribe each complete row left to right, including punctuation. Ignore every smaller ruby above or below the main text; do not include it in lines. Set unreadable=true only when a visible main-text glyph was replaced by �.{}\nMandatory language-specific main-text guidance: {}",
        preset.display_name,
        preset.code,
        row_hint(expected_main_rows),
        preset.main_text_instruction,
    )
}

pub fn ruby_recognition(preset: &LanguagePreset, lines: &[String]) -> String {
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "line_index": index + 1, "text": text }))
        .collect::<Vec<_>>();
    format!(
        "The expected subtitle language is {} ({}).\nThis is ruby recognition. The large main subtitle text has already been recognized and normalized. Inspect only visibly smaller annotation text aligned above or below the supplied main lines. Do not retranscribe or correct the main lines. For each visible annotation, line_index is the 1-based target main line, base is the exact contiguous substring from that supplied line, base_occurrence identifies which occurrence of that substring is annotated, text is the small visible annotation, and position is over or under. Return an empty annotations array when no small aligned annotation is visibly present. Set unreadable=true only when a visible annotation glyph was replaced by �. Recognized main lines: {}.\nMandatory language-specific annotation guidance: {}",
        preset.display_name,
        preset.code,
        serde_json::to_string(&numbered).expect("numbered lines serialize"),
        preset.annotation_instruction,
    )
}

pub fn whole_cue_style(preset: &LanguagePreset, lines: &[String]) -> String {
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "line_index": index + 1, "text": text }))
        .collect::<Vec<_>>();
    format!(
        "The expected subtitle language is {} ({}).\nThis is whole-Cue style recognition. Ignore transcription and annotation text. First decide whether the entire visible large main subtitle uses a consistently right-slanted italic design. Return italic=true only when every large main-text row and every visible main glyph is consistently italic; otherwise return false. Then classify the uniform main-text foreground color. Return default for white, near-white, mixed, dim antialiasing edges, outlines, shadows, or any ambiguous color. Return a named non-white color only when the glyph interiors are clearly and consistently that color across the complete Cue. Do not infer style or color from content, symbols, script, outlines, shadows, or the background. Do not classify individual lines or substrings. Recognized main lines: {}",
        preset.display_name,
        preset.code,
        serde_json::to_string(&numbered).expect("numbered lines serialize")
    )
}

pub fn retry(stage: &str, error: &str) -> String {
    format!(
        "Your previous {stage} response was rejected by deterministic validation: {error}. Re-read the image and return a corrected response matching the schema. Do not repeat the invalid value."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages;

    #[test]
    fn japanese_details_come_from_the_japanese_preset() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let english = languages::resolve("eng").expect("English preset");
        let combined = combined_recognition(japanese, Some(1));
        let main = main_text_recognition(japanese, Some(1));
        let ruby = ruby_recognition(japanese, &["甲かな乙".to_owned()]);

        assert!(combined.contains("okurigana"));
        assert!(ruby.contains("visual horizontal alignment"));
        assert!(ruby.contains("under its horizontal center"));
        assert!(ruby.contains("mixed kanji-kana text"));
        assert!(ruby.contains("kana-only base"));
        assert!(ruby.contains("standalone symbols as the sole base"));
        assert!(ruby.contains("never to expand the base"));
        assert!(ruby.contains("Latin letters and digits"));
        assert!(!combined_recognition(english, Some(1)).contains("okurigana"));
        assert!(combined.ends_with(japanese.annotation_instruction));
        assert!(main.ends_with(japanese.main_text_instruction));
        assert!(ruby.ends_with(japanese.annotation_instruction));
    }

    #[test]
    fn prompt_uses_the_canonical_language_identity() {
        let french = languages::resolve("fre").expect("French preset");
        let prompt = main_text_recognition(french, Some(2));

        assert!(prompt.contains("French (fra)"));
        assert!(prompt.contains("exactly 2 lines items"));
        assert!(prompt.contains("diacritics"));
    }

    #[test]
    fn split_prompts_assign_text_and_annotations_to_distinct_stages() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let main = main_text_recognition(japanese, Some(1));
        let ruby = ruby_recognition(japanese, &["甲かな乙".to_owned()]);

        assert!(main.contains("Ignore every smaller ruby"));
        assert!(!main.contains("base_occurrence"));
        assert!(ruby.contains("Do not retranscribe"));
        assert!(ruby.contains(r#""text":"甲かな乙""#));
    }
}
