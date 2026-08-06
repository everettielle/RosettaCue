use serde_json::json;

pub const PROMPT_VERSION: &str = "subtitle-ocr-v1";

pub const SYSTEM_PROMPT: &str = r"You are a literal OCR engine for Blu-ray subtitle images.
Transcribe only characters that are visibly present. Never translate, paraphrase,
correct grammar, complete a sentence, or add an explanation. Preserve the main
subtitle's punctuation and line breaks. If a main-text character cannot be read,
write the replacement character � instead of guessing.

Treat large main text and small aligned annotations as different layers. Small
text immediately above or below a main-text range is ruby annotation, not an
additional main subtitle line.";

pub const JAPANESE_INSTRUCTION: &str = r"Use Japanese full-width punctuation and symbols. Convert ASCII or halfwidth symbols such as ()!? punctuation, quotes, and brackets to their full-width forms, but keep Latin letters and digits unchanged. Use the full-width Japanese centered ellipsis ⋯ (U+22EF, LaTeX \cdots), never … or three ASCII/fullwidth periods. Use the Japanese prolonged sound mark ー (U+30FC) for an ambiguous long horizontal sound mark; normalize —, ―, and halfwidth ｰ to ー. Never emit C0/C1 control characters.";

pub fn main_text(language: &str, expected_main_rows: Option<usize>) -> String {
    let row_hint = expected_main_rows.map_or_else(String::new, |count| {
        format!(
            "\nDeterministic bitmap analysis found exactly {count} likely large main-text rows. Return exactly {count} lines items. Small ruby rows are not included in this count."
        )
    });
    format!(
        "The expected subtitle language is {language}.\nLanguage preference: {JAPANESE_INSTRUCTION}\nThis is pass 1. Transcribe only the large main subtitle rows. Create exactly one lines item for every main-text row, ordered top to bottom. Carefully scan the full image from top to bottom before answering; do not stop after the first row. Transcribe each complete row left to right, including punctuation. Exclude all smaller text aligned above or below a main row; it will be processed separately. Set unreadable=true only when a visible main glyph was replaced by �.{row_hint}"
    )
}

pub fn annotations(language: &str, lines: &[String]) -> String {
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "line_index": index + 1, "text": text }))
        .collect::<Vec<_>>();
    format!(
        "The expected subtitle language is {language}.\nLanguage preference: {JAPANESE_INSTRUCTION}\nThis is pass 2. Inspect only small text spatially aligned over or under the recognized main lines below. Do not return large main text as annotation. For each annotation, line_index is the 1-based target main line, base is the exact contiguous substring it annotates, base_occurrence is 1 for the first occurrence of that substring or 2 for the second, text is the small visible annotation, and position is over or under. For Japanese okurigana, exclude unannotated kana from base: small つかさど over 司る has base 司. Small ボイシス below “Voices” has base Voices and position under. Return an empty annotations array when none exist.\nRecognized main lines: {}",
        serde_json::to_string(&numbered).expect("numbered lines serialize")
    )
}

pub fn whole_cue_style(language: &str, lines: &[String]) -> String {
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "line_index": index + 1, "text": text }))
        .collect::<Vec<_>>();
    format!(
        "The expected subtitle language is {language}.\nThis is pass 3. Ignore transcription and ruby text. Decide only whether the entire visible large main subtitle uses a consistently right-slanted italic design. Return italic=true only when every large main-text row and every visible main glyph is consistently italic. If any main portion is upright, mixed, or ambiguous, return italic=false. Do not classify individual lines or substrings. Do not infer style from song symbols, dialogue content, speaker labels, or Latin versus Japanese script. Recognized main lines: {}",
        serde_json::to_string(&numbered).expect("numbered lines serialize")
    )
}

pub fn retry(stage: &str, error: &str) -> String {
    format!(
        "Your previous {stage} response was rejected by deterministic validation: {error}. Re-read the image and return a corrected response matching the schema. Do not repeat the invalid value."
    )
}
