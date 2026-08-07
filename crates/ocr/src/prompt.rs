use std::fmt::Write;

use rosettacue_domain::WritingMode;
use serde_json::json;

use crate::languages::LanguagePreset;

pub const PROMPT_VERSION: &str = "subtitle-ocr-v7";

pub const SYSTEM_PROMPT: &str = r"You are a literal OCR engine for Blu-ray subtitle images.
Transcribe only characters that are visibly present. Never translate, paraphrase,
correct grammar, complete a sentence, or add an explanation. Preserve the main
subtitle's punctuation and line breaks. If a main-text character cannot be read,
write the replacement character � instead of guessing. Never include C0 or C1
control characters in the response.

Treat large main text and small aligned annotations as different layers. Small
text running alongside a main-text range is ruby annotation, not an additional
main subtitle line.";

/// A stage prompt split into a cacheable prefix and per-Cue content.
///
/// `stable` must be byte-identical for every block processed at the same stage,
/// language, writing mode, and pipeline mode: providers place a prompt-cache
/// breakpoint at its end, so a single varying byte there costs a cache write on
/// every block instead of a read. Everything that changes per block belongs in
/// `variable`.
///
/// Writing mode is deliberately part of the cached half. It has two values, so
/// at worst a stage keeps two cache entries and both still hit; moving the
/// direction wording into `variable` would break prefix sharing outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagePrompt {
    pub stable: String,
    pub variable: String,
}

/// What the deterministic analysis found about one block.
#[derive(Debug, Clone, Default)]
pub struct BlockPrompt {
    pub writing_mode: WritingMode,
    /// Rows in horizontal writing, columns in vertical writing.
    pub expected_units: Option<usize>,
    /// Characters expected in each unit. Approximate by construction.
    pub expected_glyphs: Vec<u32>,
    /// Set only on the one corrective pass after a glyph-count mismatch.
    pub glyph_correction: Option<String>,
}

/// The words a stage prompt needs in order to describe one writing direction.
///
/// Everything a direction changes about a prompt is a noun or an order, so the
/// two directions share one set of sentences rather than two sets of prose that
/// have to be kept in step.
#[derive(Debug, Clone, Copy)]
struct Direction {
    layout: &'static str,
    unit: &'static str,
    units: &'static str,
    order: &'static str,
    ruby: &'static str,
    ruby_values: &'static str,
}

const HORIZONTAL: Direction = Direction {
    layout: "This image is one horizontal text block cropped out of a subtitle cue, not the whole cue. Characters run left to right within a row, and rows stack top to bottom.",
    unit: "row",
    units: "rows",
    order: "top to bottom",
    ruby: "Ruby sits directly above or below the main-text range it annotates.",
    ruby_values: "over for above and under for below",
};

const VERTICAL: Direction = Direction {
    layout: "This image is one vertical (縦書き) text block cropped out of a subtitle cue, not the whole cue. Characters run top to bottom within a column, and columns stack right to left.",
    unit: "column",
    units: "columns",
    order: "right to left",
    ruby: "Ruby sits directly to the right or to the left of the column range it annotates. It is never above or below it.",
    ruby_values: "right for the right-hand side and left for the left-hand side",
};

const fn direction(writing_mode: WritingMode) -> Direction {
    match writing_mode {
        WritingMode::HorizontalTb => HORIZONTAL,
        WritingMode::VerticalRl => VERTICAL,
    }
}

/// The ruby placement values the schema accepts for a writing direction.
///
/// Vertical ruby is asked for as right/left because that is what the model can
/// see; the validator folds those onto the stored, direction-relative Over and
/// Under. Asking a model about 縦書き and then demanding "over" invites it to
/// guess which way "over" points.
#[must_use]
pub const fn ruby_placement_values(writing_mode: WritingMode) -> [&'static str; 2] {
    match writing_mode {
        WritingMode::HorizontalTb => ["over", "under"],
        WritingMode::VerticalRl => ["right", "left"],
    }
}

fn unit_hint(direction: Direction, block: &BlockPrompt) -> String {
    let mut hint = match block.expected_units {
        Some(count) => format!(
            "Deterministic bitmap analysis found exactly {count} large main-text {} in this block. Return exactly {count} lines items. Small ruby {} are not included in this count.",
            direction.units, direction.units
        ),
        None => format!(
            "Deterministic bitmap analysis produced no {} estimate for this block. Infer the number of large main-text {} from the image.",
            direction.unit, direction.units
        ),
    };
    if !block.expected_glyphs.is_empty() {
        let _ = write!(
            hint,
            "\nApproximate character count per {}, in order: {}. This is measured from ink width and is only a guide — transcribe exactly what is visible rather than padding or trimming to match it.",
            direction.unit,
            serde_json::to_string(&block.expected_glyphs).expect("glyph counts serialize")
        );
    }
    if let Some(correction) = &block.glyph_correction {
        hint.push('\n');
        hint.push_str(correction);
    }
    hint
}

/// The message sent on the single corrective pass for a glyph-count mismatch.
#[must_use]
pub fn glyph_correction(
    writing_mode: WritingMode,
    unit: usize,
    expected: u32,
    actual: usize,
) -> String {
    let direction = direction(writing_mode);
    format!(
        "Your previous transcription of {} {unit} held {actual} characters where bitmap analysis measured about {expected}. Re-read that {} carefully for characters you skipped or invented. If {actual} is what is actually there, return it again unchanged.",
        direction.unit, direction.unit
    )
}

fn recognized_lines(direction: Direction, lines: &[String]) -> String {
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "line_index": index + 1, "text": text }))
        .collect::<Vec<_>>();
    format!(
        "Recognized main {}: {}",
        direction.units,
        serde_json::to_string(&numbered).expect("numbered lines serialize")
    )
}

pub fn combined_recognition(preset: &LanguagePreset, block: &BlockPrompt) -> StagePrompt {
    let direction = direction(block.writing_mode);
    StagePrompt {
        stable: format!(
            "The expected subtitle language is {} ({}).\n{}\nThis is combined character and ruby recognition. In one response, transcribe every large main subtitle {unit} and every small ruby or furigana annotation aligned with it. Create exactly one lines item for every main-text {unit}, ordered {order}. Carefully scan the full image before answering and transcribe each complete {unit} in reading order, including punctuation. Do not include small annotation text in lines. {ruby} For each annotation, line_index is the 1-based target main {unit}, base is the exact contiguous substring it annotates, base_occurrence identifies which occurrence of that substring is annotated, text is the small visible annotation, and position is {ruby_values}. Return an empty annotations array when none exist. Set unreadable=true when any visible main or annotation glyph was replaced by �.\nThe final user message states the deterministic {unit} estimate for the block in the attached image.\nMandatory language-specific main-text guidance: {}\nMandatory language-specific annotation guidance: {}",
            preset.display_name,
            preset.code,
            direction.layout,
            preset.main_text_instruction,
            preset.annotation_instruction,
            unit = direction.unit,
            order = direction.order,
            ruby = direction.ruby,
            ruby_values = direction.ruby_values,
        ),
        variable: unit_hint(direction, block),
    }
}

pub fn main_text_recognition(preset: &LanguagePreset, block: &BlockPrompt) -> StagePrompt {
    let direction = direction(block.writing_mode);
    StagePrompt {
        stable: format!(
            "The expected subtitle language is {} ({}).\n{}\nThis is main-text character recognition. Transcribe only the large main subtitle {units}, ordered {order}, one lines item per {unit}. Carefully scan the full image and transcribe each complete {unit} in reading order, including punctuation. Ignore every smaller ruby beside the main text; do not include it in lines. Set unreadable=true only when a visible main-text glyph was replaced by �.\nThe final user message states the deterministic {unit} estimate for the block in the attached image.\nMandatory language-specific main-text guidance: {}",
            preset.display_name,
            preset.code,
            direction.layout,
            preset.main_text_instruction,
            unit = direction.unit,
            units = direction.units,
            order = direction.order,
        ),
        variable: unit_hint(direction, block),
    }
}

pub fn ruby_recognition(
    preset: &LanguagePreset,
    writing_mode: WritingMode,
    lines: &[String],
) -> StagePrompt {
    let direction = direction(writing_mode);
    StagePrompt {
        stable: format!(
            "The expected subtitle language is {} ({}).\n{}\nThis is ruby recognition. The large main subtitle text has already been recognized and normalized. Inspect only visibly smaller annotation text aligned with the supplied main {units}. Do not retranscribe or correct them. {ruby} For each visible annotation, line_index is the 1-based target main {unit}, base is the exact contiguous substring from that supplied {unit}, base_occurrence identifies which occurrence of that substring is annotated, text is the small visible annotation, and position is {ruby_values}. Return an empty annotations array when no small aligned annotation is visibly present. Set unreadable=true only when a visible annotation glyph was replaced by �.\nThe final user message supplies the recognized main {units} for the block in the attached image.\nMandatory language-specific annotation guidance: {}",
            preset.display_name,
            preset.code,
            direction.layout,
            preset.annotation_instruction,
            unit = direction.unit,
            units = direction.units,
            ruby = direction.ruby,
            ruby_values = direction.ruby_values,
        ),
        variable: recognized_lines(direction, lines),
    }
}

/// Style recognition for one block.
///
/// Vertical blocks are never asked about italic. Italic is defined by a slant
/// along the horizontal baseline, an axis that does not exist in 縦書き, and
/// the instruction that works horizontally misfires there. The pipeline records
/// that the question was not asked rather than pretending to an answer.
pub fn block_style(
    preset: &LanguagePreset,
    writing_mode: WritingMode,
    lines: &[String],
) -> StagePrompt {
    let direction = direction(writing_mode);
    let slant = if writing_mode.is_vertical() {
        String::new()
    } else {
        " First decide whether the entire visible large main subtitle uses a consistently right-slanted italic design. Return italic=true when the glyph is consistently italic; otherwise return false. Then".to_owned()
    };
    StagePrompt {
        stable: format!(
            "The expected subtitle language is {} ({}).\n{}\nThis is block style recognition. Ignore transcription and annotation text.{slant} classify the uniform main-text foreground color. Return default for white, near-white, mixed, dim antialiasing edges, outlines, shadows, or any ambiguous color. Return a named non-white color only when the glyph interiors are clearly and consistently that color across the complete block. Do not infer style or color from content, symbols, script, outlines, shadows, or the background. Do not classify individual {units} or substrings.\nThe final user message supplies the recognized main {units} for the block in the attached image.",
            preset.display_name,
            preset.code,
            direction.layout,
            units = direction.units,
        ),
        variable: recognized_lines(direction, lines),
    }
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

    fn horizontal(units: Option<usize>) -> BlockPrompt {
        BlockPrompt {
            writing_mode: WritingMode::HorizontalTb,
            expected_units: units,
            ..BlockPrompt::default()
        }
    }

    fn vertical(units: Option<usize>) -> BlockPrompt {
        BlockPrompt {
            writing_mode: WritingMode::VerticalRl,
            expected_units: units,
            ..BlockPrompt::default()
        }
    }

    #[test]
    fn japanese_details_come_from_the_japanese_preset() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let english = languages::resolve("eng").expect("English preset");
        let combined = combined_recognition(japanese, &horizontal(Some(1)));
        let main = main_text_recognition(japanese, &horizontal(Some(1)));
        let ruby = ruby_recognition(
            japanese,
            WritingMode::HorizontalTb,
            &["甲かな乙".to_owned()],
        );

        assert!(combined.stable.contains("okurigana"));
        assert!(ruby.stable.contains("visual horizontal alignment"));
        assert!(ruby.stable.contains("mixed kanji-kana text"));
        assert!(ruby.stable.contains("kana-only base"));
        assert!(ruby.stable.contains("never to expand the base"));
        assert!(
            !combined_recognition(english, &horizontal(Some(1)))
                .stable
                .contains("okurigana")
        );
        assert!(combined.stable.ends_with(japanese.annotation_instruction));
        assert!(main.stable.ends_with(japanese.main_text_instruction));
        assert!(ruby.stable.ends_with(japanese.annotation_instruction));
    }

    #[test]
    fn prompt_uses_the_canonical_language_identity() {
        let french = languages::resolve("fre").expect("French preset");
        let prompt = main_text_recognition(french, &horizontal(Some(2)));

        assert!(prompt.stable.contains("French (fra)"));
        assert!(prompt.stable.contains("diacritics"));
        assert!(prompt.variable.contains("exactly 2 lines items"));
    }

    #[test]
    fn split_prompts_assign_text_and_annotations_to_distinct_stages() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let main = main_text_recognition(japanese, &horizontal(Some(1)));
        let ruby = ruby_recognition(
            japanese,
            WritingMode::HorizontalTb,
            &["甲かな乙".to_owned()],
        );

        assert!(main.stable.contains("Ignore every smaller ruby"));
        assert!(!main.stable.contains("base_occurrence"));
        assert!(ruby.stable.contains("Do not retranscribe"));
        assert!(ruby.variable.contains(r#""text":"甲かな乙""#));
    }

    #[test]
    fn stable_blocks_do_not_move_with_per_block_content() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");

        let one = combined_recognition(japanese, &horizontal(Some(1)));
        let two = combined_recognition(japanese, &horizontal(Some(2)));
        assert_eq!(one.stable, two.stable);
        assert_ne!(one.variable, two.variable);

        let none = combined_recognition(japanese, &horizontal(None));
        assert_eq!(one.stable, none.stable);

        let glyphs = combined_recognition(
            japanese,
            &BlockPrompt {
                expected_glyphs: vec![4, 7],
                ..horizontal(Some(2))
            },
        );
        assert_eq!(one.stable, glyphs.stable);

        let style_a = block_style(japanese, WritingMode::HorizontalTb, &["первая".to_owned()]);
        let style_b = block_style(japanese, WritingMode::HorizontalTb, &["другая".to_owned()]);
        assert_eq!(style_a.stable, style_b.stable);
        assert_ne!(style_a.variable, style_b.variable);

        let ruby_a = ruby_recognition(japanese, WritingMode::HorizontalTb, &["甲".to_owned()]);
        let ruby_b = ruby_recognition(japanese, WritingMode::HorizontalTb, &["乙".to_owned()]);
        assert_eq!(ruby_a.stable, ruby_b.stable);
        assert_ne!(ruby_a.variable, ruby_b.variable);
    }

    #[test]
    fn writing_direction_is_part_of_the_cached_prefix() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");

        let across = main_text_recognition(japanese, &horizontal(Some(2)));
        let down = main_text_recognition(japanese, &vertical(Some(2)));

        assert_ne!(across.stable, down.stable);
        assert!(down.stable.contains("縦書き"));
        assert!(down.stable.contains("right to left"));
        assert!(down.stable.contains("column"));
        assert!(!across.stable.contains("縦書き"));

        // Two vertical blocks still share one prefix, so the extra entry is a
        // second cache entry rather than a cache miss per block.
        assert_eq!(
            down.stable,
            main_text_recognition(japanese, &vertical(Some(3))).stable
        );
    }

    #[test]
    fn vertical_ruby_is_asked_for_in_terms_the_model_can_see() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let vertical_ruby =
            ruby_recognition(japanese, WritingMode::VerticalRl, &["冷たい".to_owned()]);

        assert!(vertical_ruby.stable.contains("position is right"));
        assert!(vertical_ruby.stable.contains("never above or below"));
        assert_eq!(
            ruby_placement_values(WritingMode::VerticalRl),
            ["right", "left"]
        );
        assert_eq!(
            ruby_placement_values(WritingMode::HorizontalTb),
            ["over", "under"]
        );
    }

    #[test]
    fn vertical_style_recognition_never_asks_about_italic() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let lines = ["冷たい".to_owned()];

        assert!(
            block_style(japanese, WritingMode::HorizontalTb, &lines)
                .stable
                .contains("italic")
        );
        assert!(
            !block_style(japanese, WritingMode::VerticalRl, &lines)
                .stable
                .contains("italic")
        );
    }

    #[test]
    fn the_glyph_estimate_is_offered_as_a_guide_not_a_target() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let prompt = combined_recognition(
            japanese,
            &BlockPrompt {
                expected_glyphs: vec![10, 9],
                ..horizontal(Some(2))
            },
        );

        assert!(prompt.variable.contains("[10,9]"));
        assert!(prompt.variable.contains("only a guide"));
        assert!(prompt.variable.contains("rather than padding or trimming"));
    }

    #[test]
    fn unit_estimates_never_leak_into_the_cached_prefix() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        for stage in [
            combined_recognition(japanese, &horizontal(Some(3))),
            main_text_recognition(japanese, &horizontal(Some(3))),
            main_text_recognition(japanese, &vertical(Some(3))),
        ] {
            assert!(
                !stage.stable.contains("exactly 3"),
                "the unit estimate must stay in the per-block half"
            );
            assert!(stage.variable.contains("exactly 3"));
        }
    }

    #[test]
    fn every_stage_emits_non_empty_per_block_content() {
        let japanese = languages::resolve("jpn").expect("Japanese preset");
        let lines = ["甲".to_owned()];
        for stage in [
            combined_recognition(japanese, &horizontal(None)),
            combined_recognition(japanese, &vertical(Some(1))),
            main_text_recognition(japanese, &horizontal(None)),
            ruby_recognition(japanese, WritingMode::VerticalRl, &lines),
            block_style(japanese, WritingMode::VerticalRl, &lines),
        ] {
            assert!(
                !stage.variable.trim().is_empty(),
                "an empty user turn would be rejected by the provider"
            );
            assert!(!stage.stable.trim().is_empty());
        }
    }
}
