use super::english::{LATIN_ANNOTATION_INSTRUCTION, LATIN_MAIN_TEXT_INSTRUCTION};
use super::normalize_text;
use super::{BlockOrder, LanguagePreset};

pub(super) const PRESET: LanguagePreset = LanguagePreset::new(
    "spa",
    "Spanish",
    LATIN_MAIN_TEXT_INSTRUCTION,
    LATIN_ANNOTATION_INSTRUCTION,
    BlockOrder::LeftToRight,
    normalize_text,
    true,
);
