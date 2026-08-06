use serde_json::json;

use crate::languages::LanguagePreset;

pub const PROMPT_VERSION: &str = "subtitle-ocr-v1";

pub const SYSTEM_PROMPT: &str = r"You are a literal OCR engine for Blu-ray subtitle images.
Transcribe only characters that are visibly present. Never translate, paraphrase,
correct grammar, complete a sentence, or add an explanation. Preserve the main
subtitle's punctuation and line breaks. If a main-text character cannot be read,
write the replacement character � instead of guessing. Never include C0 or C1
control characters in the response.

Treat large main text and small aligned annotations as different layers. Small
text immediately above or below a main-text range is ruby annotation, not an
additional main subtitle line.";

pub fn main_text(preset: &LanguagePreset, expected_main_rows: Option<usize>) -> String {
    let row_hint = expected_main_rows.map_or_else(String::new, |count| {
        format!(
            "\nDeterministic bitmap analysis found exactly {count} likely large main-text rows. Return exactly {count} lines items. Small ruby rows are not included in this count."
        )
    });
    format!(
        "The expected subtitle language is {} ({}).\nLanguage guidance: {}\nThis is pass 1. Transcribe only the large main subtitle rows. Create exactly one lines item for every main-text row, ordered top to bottom. Carefully scan the full image from top to bottom before answering; do not stop after the first row. Transcribe each complete row left to right, including punctuation. Exclude all smaller text aligned above or below a main row; it will be processed separately. Set unreadable=true only when a visible main glyph was replaced by �.{row_hint}",
        preset.display_name, preset.code, preset.main_text_instruction
    )
}

pub fn annotations(preset: &LanguagePreset, lines: &[String]) -> String {
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "line_index": index + 1, "text": text }))
        .collect::<Vec<_>>();
    format!(
        "The expected subtitle language is {} ({}).\nLanguage guidance: {}\nThis is pass 2. Inspect only small text spatially aligned over or under the recognized main lines below. Do not return large main text as annotation. For each annotation, line_index is the 1-based target main line, base is the exact contiguous substring it annotates, base_occurrence is 1 for the first occurrence of that substring or 2 for the second, text is the small visible annotation, and position is over or under. Return an empty annotations array when none exist.\nRecognized main lines: {}",
        preset.display_name,
        preset.code,
        preset.annotation_instruction,
        serde_json::to_string(&numbered).expect("numbered lines serialize")
    )
}

pub fn whole_cue_style(preset: &LanguagePreset, lines: &[String]) -> String {
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "line_index": index + 1, "text": text }))
        .collect::<Vec<_>>();
    format!(
        "The expected subtitle language is {} ({}).\nThis is pass 3. Ignore transcription and annotation text. Decide only whether the entire visible large main subtitle uses a consistently right-slanted italic design. Return italic=true only when every large main-text row and every visible main glyph is consistently italic. If any main portion is upright, mixed, or ambiguous, return italic=false. Do not classify individual lines or substrings. Do not infer style from song symbols, dialogue content, speaker labels, script, or language. Recognized main lines: {}",
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

        assert!(annotations(japanese, &["司る".to_owned()]).contains("okurigana"));
        assert!(!annotations(english, &["Voices".to_owned()]).contains("okurigana"));
    }

    #[test]
    fn prompt_uses_the_canonical_language_identity() {
        let french = languages::resolve("fre").expect("French preset");
        let prompt = main_text(french, Some(2));

        assert!(prompt.contains("French (fra)"));
        assert!(prompt.contains("exactly 2 lines items"));
        assert!(prompt.contains("diacritics"));
    }
}
