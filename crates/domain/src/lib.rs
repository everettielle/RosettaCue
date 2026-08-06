use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub const PROJECT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectMetadata {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub updated_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ProjectOrigin>,
    #[serde(default)]
    pub settings: ProjectSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectSettings {
    pub ocr_language: String,
    pub target_language: String,
    #[serde(default)]
    pub proper_nouns: Vec<ProperNounMapping>,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            ocr_language: "jpn".to_owned(),
            target_language: "kor".to_owned(),
            proper_nouns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProperNounMapping {
    pub source: String,
    pub translation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectOrigin {
    pub project_id: Uuid,
    pub path: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub cloned_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectStatistics {
    pub source_count: u64,
    pub track_count: u64,
    pub cue_count: u64,
    pub ocr_completed_count: u64,
    pub reviewed_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Ocr,
    Translation,
    PgsExtraction,
}

impl JobKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Translation => "translation",
            Self::PgsExtraction => "pgs_extraction",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Paused,
    Interrupted,
    Completed,
    Failed,
    Canceled,
}

impl JobStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct JobProgress {
    pub phase: String,
    pub current: u32,
    pub total: Option<u32>,
    pub cue_id: Option<Uuid>,
    pub cue_index: Option<u32>,
    #[serde(default)]
    pub completed_cue_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ProjectJob {
    pub id: Uuid,
    pub kind: JobKind,
    pub status: JobStatus,
    pub request: serde_json::Value,
    pub progress: JobProgress,
    pub error: Option<String>,
    pub result: Option<serde_json::Value>,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    BlurayDirectory,
}

impl SourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BlurayDirectory => "bluray_directory",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BlurayTitleInfo {
    pub index: u32,
    pub playlist: String,
    pub duration_seconds: u64,
    pub chapters: u32,
    pub angles: u32,
    pub clips: u32,
    pub video_tracks: u32,
    pub audio_tracks: u32,
    pub pgs_tracks: u32,
    pub pgs_languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BlurayDiscInfo {
    pub root_path: String,
    pub display_name: String,
    pub main_title_index: u32,
    pub titles: Vec<BlurayTitleInfo>,
}

impl BlurayDiscInfo {
    #[must_use]
    pub fn main_title(&self) -> Option<&BlurayTitleInfo> {
        self.titles
            .iter()
            .find(|title| title.index == self.main_title_index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SourceMetadata {
    Bluray(BlurayDiscInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectSource {
    pub id: Uuid,
    pub kind: SourceKind,
    pub display_name: String,
    pub path: String,
    pub fingerprint: Option<String>,
    pub metadata: SourceMetadata,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub created_at: OffsetDateTime,
}

impl ProjectSource {
    #[must_use]
    pub fn from_bluray(disc: BlurayDiscInfo) -> Self {
        let fingerprint = disc.main_title().map(|title| {
            format!(
                "bdmv:{}:{}:{}:{}",
                disc.titles.len(),
                title.playlist,
                title.duration_seconds,
                title.pgs_tracks
            )
        });
        Self {
            id: Uuid::new_v4(),
            kind: SourceKind::BlurayDirectory,
            display_name: disc.display_name.clone(),
            path: disc.root_path.clone(),
            fingerprint,
            metadata: SourceMetadata::Bluray(disc),
            created_at: OffsetDateTime::now_utc(),
        }
    }
}

impl ProjectMetadata {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            created_at: now,
            updated_at: now,
            origin: None,
            settings: ProjectSettings::default(),
        }
    }

    #[must_use]
    pub fn cloned_from(original: &Self, name: impl Into<String>, path: impl Into<String>) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            created_at: now,
            updated_at: now,
            origin: Some(ProjectOrigin {
                project_id: original.id,
                path: path.into(),
                cloned_at: now,
            }),
            settings: original.settings.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OcrStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Unreviewed,
    NeedsReview,
    Approved,
}

impl OcrStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

impl ReviewStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::NeedsReview => "needs_review",
            Self::Approved => "approved",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PgsTrackMetadata {
    pub title_index: u32,
    pub playlist: String,
    pub sup_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TrackMetadata {
    Pgs(PgsTrackMetadata),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SubtitleTrack {
    pub id: Uuid,
    pub source_id: Uuid,
    pub stream_index: u32,
    pub language: Option<String>,
    pub codec: String,
    pub metadata: TrackMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CueGeometry {
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub forced: bool,
    pub inferred_end: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitlePosition {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl CueGeometry {
    /// Classifies the cue center into a deterministic 3×3 screen region.
    #[must_use]
    pub fn position(&self) -> SubtitlePosition {
        let horizontal = axis_region(self.x, self.width, self.canvas_width);
        let vertical = axis_region(self.y, self.height, self.canvas_height);
        match (vertical, horizontal) {
            (0, 0) => SubtitlePosition::TopLeft,
            (0, 1) => SubtitlePosition::TopCenter,
            (0, _) => SubtitlePosition::TopRight,
            (1, 0) => SubtitlePosition::MiddleLeft,
            (1, 1) => SubtitlePosition::MiddleCenter,
            (1, _) => SubtitlePosition::MiddleRight,
            (_, 0) => SubtitlePosition::BottomLeft,
            (_, 1) => SubtitlePosition::BottomCenter,
            (_, _) => SubtitlePosition::BottomRight,
        }
    }
}

fn axis_region(start: u32, extent: u32, canvas: u32) -> u8 {
    let center_twice = u64::from(start)
        .saturating_mul(2)
        .saturating_add(u64::from(extent));
    let canvas = u64::from(canvas);
    if center_twice.saturating_mul(3) < canvas.saturating_mul(2) {
        0
    } else if center_twice.saturating_mul(3) < canvas.saturating_mul(4) {
        1
    } else {
        2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SubtitleCue {
    pub id: Uuid,
    pub track_id: Uuid,
    pub cue_index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub image_path: String,
    pub image_sha256: String,
    pub position: SubtitlePosition,
    pub geometry: CueGeometry,
    pub ocr_status: OcrStatus,
    pub review_status: ReviewStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RubyPosition {
    Over,
    Under,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RubyAnnotation {
    pub text: String,
    pub position: RubyPosition,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Superscript,
    Subscript,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OcrSpan {
    Text {
        text: String,
        styles: Vec<TextStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    Ruby {
        base: String,
        annotations: Vec<RubyAnnotation>,
        styles: Vec<TextStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
}

impl OcrSpan {
    #[must_use]
    pub fn styles(&self) -> &[TextStyle] {
        match self {
            Self::Text { styles, .. } | Self::Ruby { styles, .. } => styles,
        }
    }

    pub fn styles_mut(&mut self) -> &mut Vec<TextStyle> {
        match self {
            Self::Text { styles, .. } | Self::Ruby { styles, .. } => styles,
        }
    }

    #[must_use]
    pub fn color(&self) -> Option<&str> {
        match self {
            Self::Text { color, .. } | Self::Ruby { color, .. } => color.as_deref(),
        }
    }

    pub fn color_mut(&mut self) -> &mut Option<String> {
        match self {
            Self::Text { color, .. } | Self::Ruby { color, .. } => color,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OcrLine {
    pub text: String,
    pub spans: Vec<OcrSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct NormalizationRecord {
    pub rule: String,
    pub field: String,
    pub line_index: u32,
    pub annotation_index: Option<u32>,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OcrDocument {
    pub prompt_version: String,
    pub provider: String,
    pub model: String,
    pub language: String,
    pub unreadable: bool,
    pub lines: Vec<OcrLine>,
    pub normalizations: Vec<NormalizationRecord>,
}

impl OcrDocument {
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CueRecognition {
    pub cue_id: Uuid,
    pub document: OcrDocument,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CueEditDocument {
    pub start_ms: u64,
    pub end_ms: u64,
    pub position: SubtitlePosition,
    pub subtitle: OcrDocument,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionAuthor {
    Ocr,
    Human,
    Translation,
}

impl RevisionAuthor {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Human => "human",
            Self::Translation => "translation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CueRevision {
    pub id: Uuid,
    pub cue_id: Uuid,
    pub author: RevisionAuthor,
    pub document: CueEditDocument,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CueReviewDecision {
    pub id: Uuid,
    pub cue_id: Uuid,
    pub revision_id: Option<Uuid>,
    pub status: ReviewStatus,
    pub note: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schemars(with = "String")]
    pub created_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_all_nine_subtitle_positions() {
        let cases = [
            (40, 40, SubtitlePosition::TopLeft),
            (140, 40, SubtitlePosition::TopCenter),
            (240, 40, SubtitlePosition::TopRight),
            (40, 140, SubtitlePosition::MiddleLeft),
            (140, 140, SubtitlePosition::MiddleCenter),
            (240, 140, SubtitlePosition::MiddleRight),
            (40, 240, SubtitlePosition::BottomLeft),
            (140, 240, SubtitlePosition::BottomCenter),
            (240, 240, SubtitlePosition::BottomRight),
        ];
        for (x, y, expected) in cases {
            let geometry = CueGeometry {
                canvas_width: 300,
                canvas_height: 300,
                x,
                y,
                width: 20,
                height: 20,
                image_width: 20,
                image_height: 20,
                forced: false,
                inferred_end: false,
            };
            assert_eq!(geometry.position(), expected);
        }
    }

    #[test]
    fn rejects_the_removed_slant_document_shape() {
        let old_line = r#"{
            "text":"字幕",
            "style":{"slant":"italic"},
            "spans":[{"type":"text","text":"字幕","style":{"slant":"italic"}}]
        }"#;
        assert!(serde_json::from_str::<OcrLine>(old_line).is_err());
    }

    #[test]
    fn defaults_project_settings_when_opening_older_metadata() {
        let mut metadata = serde_json::to_value(ProjectMetadata::new("Movie"))
            .expect("serialize project metadata");
        metadata
            .as_object_mut()
            .expect("metadata object")
            .remove("settings");

        let restored: ProjectMetadata =
            serde_json::from_value(metadata).expect("restore project metadata");

        assert_eq!(restored.settings, ProjectSettings::default());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub stage: String,
    pub path: Option<String>,
    pub message: String,
    pub codepoint: Option<String>,
}
