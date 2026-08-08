//! Deterministic layout analysis for extracted subtitle bitmaps.
//!
//! One cue can carry several spatially separated text blocks, and those blocks
//! can be written in different directions. This crate answers three questions
//! about a cue bitmap without recognizing a single character: where the blocks
//! are, which way each one runs, and how many units (rows or columns) and
//! glyphs each one should contain.
//!
//! Every answer degrades to "one horizontal block covering the whole cue" when
//! the evidence is weak, so a defect here can never do worse than skipping the
//! analysis entirely.

mod bands;
mod mask;

use std::ops::RangeInclusive;

use bands::{
    Band, RASTER_GAP, cluster, column_activity, main_bands, max_extent, median_extent, row_activity,
};
pub use mask::{Mask, crop_png, decode_mask};
use rosettacue_domain::{BlockBounds, BlockSource, WritingMode};
use serde::{Deserialize, Serialize};

/// An axis-aligned rectangle in cue-bitmap pixels.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    #[must_use]
    pub const fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }

    #[must_use]
    pub const fn bottom(self) -> u32 {
        self.y.saturating_add(self.height)
    }

    #[must_use]
    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// The rectangle as bitmap-relative bounds, ready for `CueGeometry::canvas_bounds`.
    #[must_use]
    pub const fn bounds(self) -> BlockBounds {
        BlockBounds {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }

    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: self.right().max(other.right()) - x,
            height: self.bottom().max(other.bottom()) - y,
        }
    }
}

/// How blocks that sit side by side are ordered.
///
/// Vertical Japanese reads right to left; every other supported script reads
/// left to right. The language preset owns this choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlockOrder {
    #[default]
    LeftToRight,
    RightToLeft,
}

/// The block-detection thresholds, in units the bitmap's own em resolves.
///
/// These are the numbers that decide how a cue is cut into blocks. They are
/// separated from [`LayoutOptions`] because they are the tunable half: reading
/// order is language policy and nobody adjusts it per project, while these
/// three are exactly what a user reaches for when a track's typesetting splits
/// or fuses in a way the defaults did not anticipate.
///
/// Every value is expressed against the em the analyzer measures from the
/// bitmap, so one setting holds across resolutions and font sizes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutTuning {
    /// Blank space of this many em separates two blocks.
    pub separation_em: f32,
    /// Fragments smaller than this many em² are merged into their neighbour.
    pub minimum_block_em2: f32,
    /// More blocks than this means the analysis went wrong; degrade instead.
    pub maximum_blocks: u32,
}

/// The supported range for [`LayoutTuning::separation_em`].
///
/// The floor is what keeps an ideographic space — one em of blank, plus the
/// side bearings around it — from reading as a block boundary; the ceiling is
/// wider than any cue is tall, so nothing above it can ever split.
pub const SEPARATION_EM_RANGE: RangeInclusive<f32> = 1.5..=8.0;
/// The supported range for [`LayoutTuning::minimum_block_em2`]. Zero merges nothing.
pub const MINIMUM_BLOCK_EM2_RANGE: RangeInclusive<f32> = 0.0..=8.0;
/// The supported range for [`LayoutTuning::maximum_blocks`].
pub const MAXIMUM_BLOCKS_RANGE: RangeInclusive<u32> = 1..=32;

impl Default for LayoutTuning {
    fn default() -> Self {
        Self {
            separation_em: 2.0,
            minimum_block_em2: 0.5,
            maximum_blocks: 8,
        }
    }
}

impl LayoutTuning {
    /// The tuning with every value forced into its supported range.
    ///
    /// These numbers reach the analyzer from a settings dialog and from JSON
    /// documents on disk, so they are checked rather than trusted: a zero or
    /// negative separation would cut at every blank column between glyphs, and
    /// a NaN would compare false against both ends of any range it is tested
    /// with. An out-of-range value falls back to the default rather than to the
    /// nearest bound only when it is not a number at all.
    #[must_use]
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            separation_em: clamp_f32(
                self.separation_em,
                default.separation_em,
                SEPARATION_EM_RANGE,
            ),
            minimum_block_em2: clamp_f32(
                self.minimum_block_em2,
                default.minimum_block_em2,
                MINIMUM_BLOCK_EM2_RANGE,
            ),
            maximum_blocks: self
                .maximum_blocks
                .clamp(*MAXIMUM_BLOCKS_RANGE.start(), *MAXIMUM_BLOCKS_RANGE.end()),
        }
    }
}

fn clamp_f32(value: f32, default: f32, range: RangeInclusive<f32>) -> f32 {
    if value.is_nan() {
        return default;
    }
    value.clamp(*range.start(), *range.end())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutOptions {
    pub tuning: LayoutTuning,
    pub block_order: BlockOrder,
}

impl LayoutOptions {
    #[must_use]
    pub const fn new(tuning: LayoutTuning, block_order: BlockOrder) -> Self {
        Self {
            tuning,
            block_order,
        }
    }
}

/// Why the analyzer is unsure about part of its own answer.
///
/// These are recorded rather than acted on: they are the tuning evidence the
/// survey command aggregates, and the reason a block was degraded.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum LayoutDoubt {
    NoForeground,
    AmbiguousWritingMode { block: u32 },
    AmbiguousReadingOrder { first: u32, second: u32 },
    BlockCountCapped { found: u32 },
    TinyBlockMerged { into: u32 },
}

impl LayoutDoubt {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NoForeground => "no_foreground",
            Self::AmbiguousWritingMode { .. } => "ambiguous_writing_mode",
            Self::AmbiguousReadingOrder { .. } => "ambiguous_reading_order",
            Self::BlockCountCapped { .. } => "block_count_capped",
            Self::TinyBlockMerged { .. } => "tiny_block_merged",
        }
    }
}

/// Which rule decided a block's writing mode.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModeRule {
    /// The block is far taller than it is wide, or far wider than tall.
    Aspect,
    /// One column of several rows.
    SingleColumn,
    /// One row of several columns.
    SingleRow,
    /// Nothing was decisive; horizontal was assumed.
    Fallback,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ModeEvidence {
    pub row_bands: u32,
    pub column_bands: u32,
    /// Width divided by height, in thousandths.
    pub aspect_ratio_milli: u32,
    pub rule: ModeRule,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BlockLayout {
    pub bounds: Rect,
    pub writing_mode: WritingMode,
    pub source: BlockSource,
    /// Rows for horizontal writing, columns for vertical. `None` when unsure.
    pub units: Option<u32>,
    /// The estimated full-width glyph advance.
    pub em: u32,
    /// Glyphs expected in each unit. A soft signal — see the pipeline's use of it.
    pub expected_glyphs: Vec<u32>,
    pub evidence: ModeEvidence,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CueLayout {
    pub image: Rect,
    /// Blocks in reading order. Always at least one.
    pub blocks: Vec<BlockLayout>,
    pub doubts: Vec<LayoutDoubt>,
}

impl CueLayout {
    /// Whether the analysis fell back to treating the cue as one block.
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| block.source == BlockSource::WholeCue)
    }

    /// The total rows and columns the provider should return across all blocks.
    #[must_use]
    pub fn total_units(&self) -> Option<u32> {
        self.blocks
            .iter()
            .map(|block| block.units)
            .try_fold(0_u32, |total, units| Some(total.saturating_add(units?)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("the cue bitmap could not be decoded: {0}")]
    Decode(String),
    #[error("the cue bitmap could not be encoded: {0}")]
    Encode(String),
    #[error("the cue bitmap has no pixels")]
    Empty,
    #[error("the crop rectangle falls outside the cue bitmap")]
    CropOutOfBounds,
}

/// Analyzes a cue PNG.
///
/// # Errors
///
/// Returns an error only when the PNG itself cannot be decoded. Every layout
/// uncertainty is reported through [`CueLayout::doubts`] instead.
pub fn analyze_png(png_bytes: &[u8], options: &LayoutOptions) -> Result<CueLayout, LayoutError> {
    Ok(analyze_mask(&decode_mask(png_bytes)?, options))
}

/// Analyzes a foreground mask. Pure, total, and the whole algorithm.
#[must_use]
pub fn analyze_mask(mask: &Mask, options: &LayoutOptions) -> CueLayout {
    // The thresholds may come straight from a settings dialog or a config
    // document, so they are pulled into range here rather than at the edge:
    // this is the one place every caller passes through.
    let tuning = options.tuning.clamped();
    let image = mask.area();
    let mut doubts = Vec::new();
    let Some(content) = bands::tight_bounds(mask, image) else {
        doubts.push(LayoutDoubt::NoForeground);
        return degraded(image, doubts);
    };

    let em = bootstrap_em(mask, content);
    let separation = scale(em, tuning.separation_em).max(1);
    let mut rectangles = Vec::new();
    cut(mask, content, separation, CUT_DEPTH, &mut rectangles);

    let minimum_area = u64::from(scale(em, tuning.minimum_block_em2)) * u64::from(em);
    let merged = merge_fragments(rectangles, minimum_area);
    if u32::try_from(merged.len()).unwrap_or(u32::MAX) > tuning.maximum_blocks {
        doubts.push(LayoutDoubt::BlockCountCapped {
            found: u32::try_from(merged.len()).unwrap_or(u32::MAX),
        });
        return degraded(image, doubts);
    }

    let mut candidates = merged;
    order_for_reading(&mut candidates, options.block_order);
    let mut blocks = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        for _ in 0..candidate.absorbed {
            doubts.push(LayoutDoubt::TinyBlockMerged { into: index });
        }
        blocks.push(analyze_block(mask, candidate.rect, index, &mut doubts));
    }
    doubts.extend(ambiguous_neighbours(&candidates));
    CueLayout {
        image,
        blocks,
        doubts,
    }
}

/// The recursion cap for the X-Y cut; deeper than any real subtitle needs.
const CUT_DEPTH: u32 = 6;

/// The overlap share, in thousandths, above which two blocks count as side by side.
const SIDE_BY_SIDE_MILLI: u32 = 400;
/// Overlap this close to the threshold is reported as an uncertain ordering.
const AMBIGUOUS_ORDER_MILLI: u32 = 600;

/// A cue the analyzer declined to split: one horizontal block, no unit count.
///
/// The pipeline recognizes this exactly the way it did before blocks existed,
/// which is what makes a wrong answer here harmless.
fn degraded(image: Rect, doubts: Vec<LayoutDoubt>) -> CueLayout {
    CueLayout {
        image,
        blocks: vec![BlockLayout {
            bounds: image,
            writing_mode: WritingMode::HorizontalTb,
            source: BlockSource::WholeCue,
            units: None,
            em: 0,
            expected_glyphs: Vec::new(),
            evidence: ModeEvidence {
                row_bands: 0,
                column_bands: 0,
                aspect_ratio_milli: milli_ratio(image.width, image.height),
                rule: ModeRule::Fallback,
            },
        }],
        doubts,
    }
}

/// Estimates the full-width glyph size before any block is known.
///
/// Separating blocks needs a threshold in em, but em is normally read off a
/// block — so the first estimate has to come from the whole cue, without
/// knowing which way the text runs. Band counts settle that: across the flow
/// there is one band per line of text, while along it there is one per glyph or
/// per stroke, so the axis with fewer bands is the one running across. On that
/// axis the longest band is a line of text seen edge on, which is one em.
///
/// Neither the median nor the other axis can stand in for it. Most scripts
/// break a glyph into several ink bands along the flow — the two radicals of
/// 鈴, the three dots of `…`, the stem and bowl of a Latin letter — so bands
/// there measure strokes, and both the middle and the longest of them come out
/// a fraction of an em. A separation threshold scaled from that fraction is
/// narrower than the blank an ideographic space leaves, which cuts one line of
/// dialogue into a block per phrase.
fn bootstrap_em(mask: &Mask, content: Rect) -> u32 {
    let rows = cluster(&row_activity(mask, content), content.y, RASTER_GAP);
    let columns = cluster(&column_activity(mask, content), content.x, RASTER_GAP);
    let across = if rows.len() <= columns.len() {
        &rows
    } else {
        &columns
    };
    max_extent(across).max(1)
}

fn scale(em: u32, factor: f32) -> u32 {
    let scaled = f64::from(em) * f64::from(factor);
    if !scaled.is_finite() || scaled <= 0.0 {
        return 0;
    }
    if scaled >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        scaled.round() as u32
    }
}

fn milli_ratio(width: u32, height: u32) -> u32 {
    if height == 0 {
        return u32::MAX;
    }
    u32::try_from(u64::from(width) * 1000 / u64::from(height)).unwrap_or(u32::MAX)
}

/// Recursively splits an area wherever a blank run is wider than `separation`.
fn cut(mask: &Mask, area: Rect, separation: u32, depth: u32, out: &mut Vec<Rect>) {
    let Some(area) = bands::tight_bounds(mask, area) else {
        return;
    };
    if depth == 0 {
        out.push(area);
        return;
    }
    let vertical = cluster(&column_activity(mask, area), area.x, separation);
    if vertical.len() > 1 {
        for band in vertical {
            cut(
                mask,
                Rect {
                    x: band.start,
                    width: band.extent(),
                    ..area
                },
                separation,
                depth - 1,
                out,
            );
        }
        return;
    }
    let horizontal = cluster(&row_activity(mask, area), area.y, separation);
    if horizontal.len() > 1 {
        for band in horizontal {
            cut(
                mask,
                Rect {
                    y: band.start,
                    height: band.extent(),
                    ..area
                },
                separation,
                depth - 1,
                out,
            );
        }
        return;
    }
    out.push(area);
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    rect: Rect,
    absorbed: u32,
}

/// Folds specks — a stray punctuation mark, a decoding artefact — into the
/// nearest real block so they never become blocks of their own.
fn merge_fragments(rectangles: Vec<Rect>, minimum_area: u64) -> Vec<Candidate> {
    let mut candidates = rectangles
        .into_iter()
        .map(|rect| Candidate { rect, absorbed: 0 })
        .collect::<Vec<_>>();
    loop {
        if candidates.len() < 2 {
            return candidates;
        }
        let Some(fragment) = candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.rect.area() < minimum_area)
            .min_by_key(|(_, candidate)| candidate.rect.area())
            .map(|(index, _)| index)
        else {
            return candidates;
        };
        let removed = candidates.remove(fragment);
        let Some(host) = candidates
            .iter()
            .enumerate()
            .min_by_key(|(_, candidate)| centre_distance(candidate.rect, removed.rect))
            .map(|(index, _)| index)
        else {
            return candidates;
        };
        candidates[host].rect = candidates[host].rect.union(removed.rect);
        candidates[host].absorbed += removed.absorbed + 1;
    }
}

fn centre_distance(left: Rect, right: Rect) -> u64 {
    let centre = |rect: Rect| {
        (
            u64::from(rect.x) * 2 + u64::from(rect.width),
            u64::from(rect.y) * 2 + u64::from(rect.height),
        )
    };
    let (left_x, left_y) = centre(left);
    let (right_x, right_y) = centre(right);
    left_x.abs_diff(right_x) + left_y.abs_diff(right_y)
}

/// Sorts blocks the way a reader takes them: stacked blocks top to bottom, and
/// blocks that share a band of the screen in the script's horizontal direction.
fn order_for_reading(candidates: &mut [Candidate], order: BlockOrder) {
    candidates.sort_by(|left, right| {
        if vertical_overlap_milli(left.rect, right.rect) < SIDE_BY_SIDE_MILLI {
            return left.rect.y.cmp(&right.rect.y);
        }
        match order {
            BlockOrder::LeftToRight => left.rect.x.cmp(&right.rect.x),
            BlockOrder::RightToLeft => right.rect.right().cmp(&left.rect.right()),
        }
    });
}

/// Flags neighbouring blocks whose overlap sits right at the stacked/side-by-side
/// boundary, where the reading order genuinely could go either way.
fn ambiguous_neighbours(candidates: &[Candidate]) -> Vec<LayoutDoubt> {
    candidates
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| {
            let overlap = vertical_overlap_milli(pair[0].rect, pair[1].rect);
            (SIDE_BY_SIDE_MILLI..=AMBIGUOUS_ORDER_MILLI)
                .contains(&overlap)
                .then(|| LayoutDoubt::AmbiguousReadingOrder {
                    first: u32::try_from(index).unwrap_or(u32::MAX),
                    second: u32::try_from(index + 1).unwrap_or(u32::MAX),
                })
        })
        .collect()
}

/// How much of the shorter block's height the two blocks share, in thousandths.
fn vertical_overlap_milli(left: Rect, right: Rect) -> u32 {
    let top = left.y.max(right.y);
    let bottom = left.bottom().min(right.bottom());
    let shorter = left.height.min(right.height);
    if bottom <= top || shorter == 0 {
        return 0;
    }
    u32::try_from(u64::from(bottom - top) * 1000 / u64::from(shorter)).unwrap_or(1000)
}

fn analyze_block(
    mask: &Mask,
    rect: Rect,
    index: u32,
    doubts: &mut Vec<LayoutDoubt>,
) -> BlockLayout {
    let rows = cluster(&row_activity(mask, rect), rect.y, RASTER_GAP);
    let columns = cluster(&column_activity(mask, rect), rect.x, RASTER_GAP);
    let aspect_ratio_milli = milli_ratio(rect.width, rect.height);
    let (writing_mode, rule) = judge_writing_mode(aspect_ratio_milli, rows.len(), columns.len());
    if rule == ModeRule::Fallback {
        doubts.push(LayoutDoubt::AmbiguousWritingMode { block: index });
    }
    // Units run across the flow — columns in vertical writing, rows in
    // horizontal writing — so those bands stay one glyph apart and can be
    // counted. Glyph size is their ink extent plus the ordinary blank run
    // between glyphs along the flow: ink alone is the advance minus that gap,
    // and the shortfall compounds once it is divided into a whole line.
    let (across, along) = if writing_mode.is_vertical() {
        (&columns, &rows)
    } else {
        (&rows, &columns)
    };
    let gap = bands::median_gap(along);
    let em = median_extent(across).saturating_add(gap).max(1);
    let unit_bands = main_bands(across);
    let units = u32::try_from(unit_bands.len())
        .ok()
        .filter(|count| (1..=MAXIMUM_UNITS).contains(count));
    let expected_glyphs = units.map_or_else(Vec::new, |_| {
        unit_bands
            .iter()
            .map(|band| glyph_estimate(mask, rect, *band, writing_mode, em, gap))
            .collect()
    });
    BlockLayout {
        bounds: rect,
        writing_mode,
        source: BlockSource::Detected,
        units,
        em,
        expected_glyphs,
        evidence: ModeEvidence {
            row_bands: u32::try_from(rows.len()).unwrap_or(u32::MAX),
            column_bands: u32::try_from(columns.len()).unwrap_or(u32::MAX),
            aspect_ratio_milli,
            rule,
        },
    }
}

/// A subtitle block with more rows or columns than this was misread.
const MAXIMUM_UNITS: u32 = 8;

/// A block this much wider than tall is horizontal; this much taller is vertical.
const WIDE_MILLI: u32 = 2000;
const TALL_MILLI: u32 = 500;

/// Decides which way a block runs.
///
/// Shape comes first because it is the one signal that cannot be inverted: a
/// box five times wider than it is tall is not one vertical column, whatever
/// the band counts say. Band counts only settle blocks that are close to square.
fn judge_writing_mode(
    aspect_ratio_milli: u32,
    row_bands: usize,
    column_bands: usize,
) -> (WritingMode, ModeRule) {
    if aspect_ratio_milli >= WIDE_MILLI {
        return (WritingMode::HorizontalTb, ModeRule::Aspect);
    }
    if aspect_ratio_milli <= TALL_MILLI {
        return (WritingMode::VerticalRl, ModeRule::Aspect);
    }
    if column_bands == 1 && row_bands > 1 {
        return (WritingMode::VerticalRl, ModeRule::SingleColumn);
    }
    if row_bands == 1 && column_bands > 1 {
        return (WritingMode::HorizontalTb, ModeRule::SingleRow);
    }
    (WritingMode::HorizontalTb, ModeRule::Fallback)
}

/// Estimates the glyphs in one unit from its length along the flow axis.
///
/// Ink of `n` glyphs runs `n × em − gap`, so the count comes from the whole
/// length divided by the advance — never from counting the gaps themselves,
/// which merge and vanish wherever punctuation or Latin text appears.
fn glyph_estimate(
    mask: &Mask,
    block: Rect,
    band: Band,
    writing_mode: WritingMode,
    em: u32,
    gap: u32,
) -> u32 {
    let unit = if writing_mode.is_vertical() {
        Rect {
            x: band.start,
            width: band.extent(),
            ..block
        }
    } else {
        Rect {
            y: band.start,
            height: band.extent(),
            ..block
        }
    };
    let span = bands::tight_bounds(mask, unit).map_or(0, |tight| {
        if writing_mode.is_vertical() {
            tight.height
        } else {
            tight.width
        }
    });
    (span.saturating_add(gap).saturating_add(em / 2) / em).max(1)
}

#[cfg(test)]
mod tests;
