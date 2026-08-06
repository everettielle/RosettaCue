use super::{LanguagePreset, normalize_text};

pub(super) const LATIN_MAIN_TEXT_INSTRUCTION: &str = r"Preserve Latin letters, capitalization, diacritics, punctuation, contractions, and spacing exactly as visible. Do not correct spelling or expand abbreviations. Keep language-native characters rather than replacing them with unaccented ASCII.";

pub(super) const LATIN_ANNOTATION_INSTRUCTION: &str = r"Small aligned text is uncommon in Latin-script subtitles. Return it only when visibly distinct smaller text is spatially aligned with an exact base substring. Do not reinterpret accents, punctuation, or another main-text row as annotation.";

pub(super) const PRESET: LanguagePreset = LanguagePreset::new(
    "eng",
    "English",
    LATIN_MAIN_TEXT_INSTRUCTION,
    LATIN_ANNOTATION_INSTRUCTION,
    normalize_text,
    true,
);
