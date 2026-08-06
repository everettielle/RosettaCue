use super::{LanguagePreset, normalize_text};

const MAIN_TEXT_INSTRUCTION: &str = r"Preserve Hangul syllable blocks, visibly isolated Jamo, Latin letters, digits, punctuation, and spacing exactly as shown. Do not insert spaces or infer omitted particles or endings. Keep clearly composed Hangul as composed syllables.";

const ANNOTATION_INSTRUCTION: &str = r"Return small Hangul or Latin pronunciation or translation text only when it is visibly aligned over or under an exact base substring. Do not treat punctuation, detached Jamo within the main glyph row, or another main-text row as annotation.";

pub(super) const PRESET: LanguagePreset = LanguagePreset::new(
    "kor",
    "Korean",
    MAIN_TEXT_INSTRUCTION,
    ANNOTATION_INSTRUCTION,
    normalize_text,
    true,
);
