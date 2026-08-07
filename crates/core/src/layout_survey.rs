//! Aggregate layout analysis over a project's cues.
//!
//! Recognition costs money and time; counting how many cues actually hold more
//! than one block, or run vertically, does not. This is the measurement that
//! says how much the block pipeline is worth on a given track, and it is also
//! where the analyzer's thresholds get their tuning evidence.

use std::collections::BTreeMap;

use rosettacue_domain::{BlockBounds, BlockSource, SubtitleCue, WritingMode};
use rosettacue_layout::{BlockLayout, CueLayout, ModeRule};
use serde::Serialize;
use uuid::Uuid;

use crate::ProjectError;

#[derive(Debug, Clone, Default, Serialize)]
pub struct LayoutSurvey {
    pub cue_count: u32,
    /// Cues whose bitmap could not be read or decoded.
    pub failed_cues: u32,
    /// How many cues yielded 1, 2, 3… blocks.
    pub cues_by_block_count: BTreeMap<u32, u32>,
    /// How many blocks came out horizontal and how many vertical.
    pub blocks_by_writing_mode: BTreeMap<String, u32>,
    /// Cues holding at least one vertical block.
    pub vertical_cues: u32,
    /// Cues holding both a vertical and a horizontal block.
    pub mixed_direction_cues: u32,
    /// Cues the analyzer declined to split, which recognize as they did before.
    pub degraded_cues: u32,
    /// How often each doubt was raised, by code.
    pub doubts: BTreeMap<String, u32>,
    /// Every cue that is not one confident horizontal block.
    pub notable_cues: Vec<CueLayoutSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CueLayoutSummary {
    pub cue_id: Uuid,
    pub cue_index: u32,
    /// Rows and columns the provider is expected to return in total.
    pub total_units: Option<u32>,
    pub blocks: Vec<CueLayoutBlock>,
    pub doubts: Vec<String>,
    /// Set when this cue could not be analyzed at all.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CueLayoutBlock {
    /// Canvas coordinates, the same frame the renderer and the export use.
    pub bounds: BlockBounds,
    pub writing_mode: WritingMode,
    pub source: BlockSource,
    pub units: Option<u32>,
    pub em: u32,
    pub expected_glyphs: Vec<u32>,
    pub rule: ModeRule,
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutSurveyError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Ocr(#[from] rosettacue_ocr::OcrError),
}

/// Accumulates one cue at a time so a single unreadable bitmap cannot end the run.
#[derive(Debug, Default)]
pub(crate) struct SurveyBuilder {
    survey: LayoutSurvey,
}

impl SurveyBuilder {
    pub(crate) fn add_failure(&mut self, cue: &SubtitleCue, error: &impl std::fmt::Display) {
        self.survey.cue_count = self.survey.cue_count.saturating_add(1);
        self.survey.failed_cues = self.survey.failed_cues.saturating_add(1);
        self.survey.notable_cues.push(CueLayoutSummary {
            cue_id: cue.id,
            cue_index: cue.cue_index,
            total_units: None,
            blocks: Vec::new(),
            doubts: Vec::new(),
            error: Some(error.to_string()),
        });
    }

    pub(crate) fn add(&mut self, cue: &SubtitleCue, layout: &CueLayout) {
        self.survey.cue_count = self.survey.cue_count.saturating_add(1);
        let block_count = u32::try_from(layout.blocks.len()).unwrap_or(u32::MAX);
        *self
            .survey
            .cues_by_block_count
            .entry(block_count)
            .or_default() += 1;
        for block in &layout.blocks {
            *self
                .survey
                .blocks_by_writing_mode
                .entry(block.writing_mode.as_str().to_owned())
                .or_default() += 1;
        }
        for doubt in &layout.doubts {
            *self
                .survey
                .doubts
                .entry(doubt.code().to_owned())
                .or_default() += 1;
        }

        let vertical = layout
            .blocks
            .iter()
            .any(|block| block.writing_mode.is_vertical());
        let horizontal = layout
            .blocks
            .iter()
            .any(|block| !block.writing_mode.is_vertical());
        if vertical {
            self.survey.vertical_cues = self.survey.vertical_cues.saturating_add(1);
        }
        if vertical && horizontal {
            self.survey.mixed_direction_cues = self.survey.mixed_direction_cues.saturating_add(1);
        }
        if layout.is_degraded() {
            self.survey.degraded_cues = self.survey.degraded_cues.saturating_add(1);
        }

        if block_count == 1 && !vertical && layout.doubts.is_empty() {
            return;
        }
        self.survey.notable_cues.push(CueLayoutSummary {
            cue_id: cue.id,
            cue_index: cue.cue_index,
            total_units: layout.total_units(),
            blocks: layout
                .blocks
                .iter()
                .map(|block| summarize(cue, block))
                .collect(),
            doubts: layout
                .doubts
                .iter()
                .map(|doubt| doubt.code().to_owned())
                .collect(),
            error: None,
        });
    }

    pub(crate) fn finish(self) -> LayoutSurvey {
        self.survey
    }
}

fn summarize(cue: &SubtitleCue, block: &BlockLayout) -> CueLayoutBlock {
    CueLayoutBlock {
        bounds: cue.geometry.canvas_bounds(block.bounds.bounds()),
        writing_mode: block.writing_mode,
        source: block.source,
        units: block.units,
        em: block.em,
        expected_glyphs: block.expected_glyphs.clone(),
        rule: block.evidence.rule,
    }
}
