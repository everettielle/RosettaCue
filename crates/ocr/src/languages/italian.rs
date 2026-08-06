use super::LanguagePreset;
use super::english::{LATIN_ANNOTATION_INSTRUCTION, LATIN_MAIN_TEXT_INSTRUCTION};
use super::normalize_text;

pub(super) const PRESET: LanguagePreset = LanguagePreset::new(
    "ita",
    "Italian",
    LATIN_MAIN_TEXT_INSTRUCTION,
    LATIN_ANNOTATION_INSTRUCTION,
    normalize_text,
    true,
);
