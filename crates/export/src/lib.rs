use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use rosettacue_domain::{
    CueEditDocument, CueGeometry, CueRevision, OcrLine, ProjectMetadata, ReviewStatus, SubtitleCue,
    SubtitlePosition, SubtitleTrack, TextStyle,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub const SUBTITLE_DOCUMENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Srt,
}

impl ExportFormat {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Srt => "srt",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportScope {
    AllRecognized,
    ApprovedOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    pub track_id: Option<Uuid>,
    pub formats: Vec<ExportFormat>,
    pub scope: ExportScope,
    pub output_directory: PathBuf,
    pub base_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleDocument {
    pub format: String,
    pub version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub exported_at: OffsetDateTime,
    pub project: ExportProject,
    pub track: ExportTrack,
    pub cues: Vec<ExportCue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProject {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTrack {
    pub id: Uuid,
    pub language: Option<String>,
    pub codec: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportCue {
    pub id: Uuid,
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub position: SubtitlePosition,
    pub geometry: CueGeometry,
    pub review_status: ReviewStatus,
    pub image_sha256: String,
    pub subtitle: rosettacue_domain::OcrDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportArtifact {
    pub format: ExportFormat,
    pub path: String,
    pub cue_count: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub track_id: Uuid,
    pub artifacts: Vec<ExportArtifact>,
    pub skipped_cues: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the project has no subtitle track to export")]
    NoTracks,
    #[error("select a subtitle track before exporting a project with multiple tracks")]
    TrackRequired,
    #[error("subtitle track {0} was not found")]
    TrackNotFound(Uuid),
    #[error("select at least one export format")]
    NoFormats,
    #[error("the export contains no recognized cues in the selected scope")]
    NoCues,
    #[error("the export output path is not a directory")]
    InvalidOutputDirectory,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Builds the canonical subtitle document and writes all requested derivatives.
///
/// # Errors
///
/// Returns an error when track selection is ambiguous, the scope has no usable
/// cues, or an output file cannot be serialized or written.
pub fn export_subtitles(
    metadata: &ProjectMetadata,
    tracks: &[SubtitleTrack],
    cues: &[SubtitleCue],
    revisions: &[CueRevision],
    options: &ExportOptions,
) -> Result<ExportResult, ExportError> {
    if options.formats.is_empty() {
        return Err(ExportError::NoFormats);
    }
    fs::create_dir_all(&options.output_directory)?;
    if !options.output_directory.is_dir() {
        return Err(ExportError::InvalidOutputDirectory);
    }
    let track = select_track(tracks, cues, options.track_id)?;
    let revision_by_cue = revisions
        .iter()
        .map(|revision| (revision.cue_id, revision))
        .collect::<HashMap<_, _>>();
    let track_cues = cues
        .iter()
        .filter(|cue| cue.track_id == track.id)
        .collect::<Vec<_>>();
    let mut exported = track_cues
        .iter()
        .filter(|cue| {
            options.scope == ExportScope::AllRecognized
                || cue.review_status == ReviewStatus::Approved
        })
        .filter_map(|cue| {
            revision_by_cue
                .get(&cue.id)
                .map(|revision| export_cue(cue, &revision.document))
        })
        .collect::<Vec<_>>();
    exported.sort_by_key(|cue| (cue.start_ms, cue.end_ms, cue.index));
    if exported.is_empty() {
        return Err(ExportError::NoCues);
    }
    let skipped_cues =
        u32::try_from(track_cues.len().saturating_sub(exported.len())).unwrap_or(u32::MAX);
    let document = SubtitleDocument {
        format: "rosettacue-subtitles".to_owned(),
        version: SUBTITLE_DOCUMENT_VERSION,
        exported_at: OffsetDateTime::now_utc(),
        project: ExportProject {
            id: metadata.id,
            name: metadata.name.clone(),
        },
        track: ExportTrack {
            id: track.id,
            language: track.language.clone(),
            codec: track.codec.clone(),
        },
        cues: exported,
    };
    let base_name = options
        .base_name
        .as_deref()
        .map(safe_file_stem)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_name(metadata, track));
    let mut artifacts = Vec::with_capacity(options.formats.len());
    for format in unique_formats(&options.formats) {
        let path = options
            .output_directory
            .join(format!("{base_name}.{}", format.extension()));
        let (contents, warnings) = match format {
            ExportFormat::Json => (serde_json::to_string_pretty(&document)?, Vec::new()),
            ExportFormat::Srt => render_srt(&document.cues),
        };
        fs::write(&path, contents)?;
        artifacts.push(ExportArtifact {
            format,
            path: path.to_string_lossy().into_owned(),
            cue_count: u32::try_from(document.cues.len()).unwrap_or(u32::MAX),
            warnings,
        });
    }
    Ok(ExportResult {
        track_id: track.id,
        artifacts,
        skipped_cues,
    })
}

fn select_track<'a>(
    tracks: &'a [SubtitleTrack],
    cues: &[SubtitleCue],
    requested: Option<Uuid>,
) -> Result<&'a SubtitleTrack, ExportError> {
    if let Some(track_id) = requested {
        return tracks
            .iter()
            .find(|track| track.id == track_id)
            .ok_or(ExportError::TrackNotFound(track_id));
    }
    let cue_track_ids = cues
        .iter()
        .map(|cue| cue.track_id)
        .collect::<std::collections::HashSet<_>>();
    let candidates = tracks
        .iter()
        .filter(|track| cue_track_ids.contains(&track.id))
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Err(ExportError::NoTracks),
        [track] => Ok(*track),
        _ => Err(ExportError::TrackRequired),
    }
}

fn export_cue(cue: &SubtitleCue, document: &CueEditDocument) -> ExportCue {
    ExportCue {
        id: cue.id,
        index: cue.cue_index,
        start_ms: document.start_ms,
        end_ms: document.end_ms,
        position: document.position,
        geometry: cue.geometry.clone(),
        review_status: cue.review_status,
        image_sha256: cue.image_sha256.clone(),
        subtitle: document.subtitle.clone(),
    }
}

fn unique_formats(formats: &[ExportFormat]) -> Vec<ExportFormat> {
    let mut unique = Vec::new();
    for format in formats {
        if !unique.contains(format) {
            unique.push(*format);
        }
    }
    unique
}

fn default_base_name(metadata: &ProjectMetadata, track: &SubtitleTrack) -> String {
    let name = safe_file_stem(&metadata.name);
    let language = track.language.as_deref().unwrap_or("und");
    format!("{name}.{language}")
}

fn safe_file_stem(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ if character.is_control() => '_',
            _ => character,
        })
        .collect()
}

fn render_srt(cues: &[ExportCue]) -> (String, Vec<String>) {
    let mut output = String::new();
    let mut warnings = Vec::new();
    for (position, cue) in cues.iter().enumerate() {
        output.push_str(&(position + 1).to_string());
        output.push('\n');
        output.push_str(&format_srt_timestamp(cue.start_ms));
        output.push_str(" --> ");
        output.push_str(&format_srt_timestamp(cue.end_ms));
        output.push('\n');
        for line in &cue.subtitle.lines {
            output.push_str(&render_srt_line(line));
            output.push('\n');
            let unsupported_styles = line.spans.iter().any(|span| {
                span.styles().iter().any(|style| {
                    matches!(
                        style,
                        TextStyle::Strikethrough | TextStyle::Superscript | TextStyle::Subscript
                    )
                })
            });
            if unsupported_styles {
                warnings.push(format!(
                    "Cue {} contains strikethrough or baseline styles; portable SRT output omits them",
                    cue.index
                ));
            }
            if line
                .spans
                .iter()
                .any(|span| matches!(span, rosettacue_domain::OcrSpan::Ruby { .. }))
            {
                warnings.push(format!(
                    "Cue {} contains ruby annotations; SRT keeps only the base text",
                    cue.index
                ));
            }
        }
        output.push('\n');
    }
    (output, warnings)
}

fn render_srt_line(line: &OcrLine) -> String {
    if line.spans.is_empty() {
        return escape_srt_text(&line.text);
    }
    line.spans
        .iter()
        .map(|span| match span {
            rosettacue_domain::OcrSpan::Text { text, styles } => render_srt_fragment(text, styles),
            rosettacue_domain::OcrSpan::Ruby { base, styles, .. } => {
                render_srt_fragment(base, styles)
            }
        })
        .collect()
}

fn render_srt_fragment(text: &str, styles: &[TextStyle]) -> String {
    let mut rendered = escape_srt_text(text);
    for (style, tag) in [
        (TextStyle::Underline, "u"),
        (TextStyle::Italic, "i"),
        (TextStyle::Bold, "b"),
    ] {
        if styles.contains(&style) {
            rendered = format!("<{tag}>{rendered}</{tag}>");
        }
    }
    rendered
}

fn escape_srt_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn format_srt_timestamp(milliseconds: u64) -> String {
    let hours = milliseconds / 3_600_000;
    let minutes = (milliseconds / 60_000) % 60;
    let seconds = (milliseconds / 1_000) % 60;
    let millis = milliseconds % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosettacue_domain::{OcrDocument, OcrSpan, RevisionAuthor, TextStyle};

    fn fixture() -> (ProjectMetadata, SubtitleTrack, SubtitleCue, CueRevision) {
        let metadata = ProjectMetadata::new("Test / Movie");
        let track_id = Uuid::new_v4();
        let track = SubtitleTrack {
            id: track_id,
            source_id: Uuid::new_v4(),
            stream_index: 0,
            language: Some("jpn".to_owned()),
            codec: "pgs".to_owned(),
            metadata: rosettacue_domain::TrackMetadata::Pgs(rosettacue_domain::PgsTrackMetadata {
                title_index: 1,
                playlist: "00001".to_owned(),
                sup_path: "assets/tracks/source.sup".to_owned(),
            }),
        };
        let geometry = CueGeometry {
            canvas_width: 1920,
            canvas_height: 1080,
            x: 700,
            y: 850,
            width: 520,
            height: 80,
            image_width: 552,
            image_height: 112,
            forced: false,
            inferred_end: false,
        };
        let cue = SubtitleCue {
            id: Uuid::new_v4(),
            track_id,
            cue_index: 48,
            start_ms: 3_723_004,
            end_ms: 3_725_678,
            image_path: "assets/cues/48.png".to_owned(),
            image_sha256: "abc".to_owned(),
            position: SubtitlePosition::BottomCenter,
            geometry,
            ocr_status: rosettacue_domain::OcrStatus::Succeeded,
            review_status: ReviewStatus::Approved,
        };
        let subtitle = OcrDocument {
            prompt_version: "test".to_owned(),
            provider: "test".to_owned(),
            model: "test".to_owned(),
            language: "jpn".to_owned(),
            unreadable: false,
            lines: vec![OcrLine {
                text: "物語＆字幕".to_owned(),
                spans: vec![OcrSpan::Text {
                    text: "物語＆字幕".to_owned(),
                    styles: vec![TextStyle::Italic],
                }],
            }],
            normalizations: Vec::new(),
        };
        let revision = CueRevision {
            id: Uuid::new_v4(),
            cue_id: cue.id,
            author: RevisionAuthor::Human,
            document: CueEditDocument {
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                position: cue.position,
                subtitle,
            },
            created_at: OffsetDateTime::now_utc(),
        };
        (metadata, track, cue, revision)
    }

    #[test]
    fn writes_canonical_json_and_srt() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let (metadata, track, cue, revision) = fixture();
        let result = export_subtitles(
            &metadata,
            std::slice::from_ref(&track),
            std::slice::from_ref(&cue),
            std::slice::from_ref(&revision),
            &ExportOptions {
                track_id: None,
                formats: vec![ExportFormat::Json, ExportFormat::Srt],
                scope: ExportScope::ApprovedOnly,
                output_directory: temporary.path().to_path_buf(),
                base_name: None,
            },
        )
        .expect("export subtitles");
        assert_eq!(result.artifacts.len(), 2);
        let json_path = temporary.path().join("Test _ Movie.jpn.json");
        let json = fs::read_to_string(json_path).expect("read JSON");
        assert!(json.contains("\"rosettacue-subtitles\""));
        assert!(json.contains("\"position\": \"bottom-center\""));
        let srt =
            fs::read_to_string(temporary.path().join("Test _ Movie.jpn.srt")).expect("read SRT");
        assert!(srt.contains("01:02:03,004 --> 01:02:05,678"));
        assert!(srt.contains("<i>物語＆字幕</i>"));
    }

    #[test]
    fn renders_portable_character_styles_as_inline_srt_markup() {
        let line = OcrLine {
            text: "通常強調通常".to_owned(),
            spans: vec![
                OcrSpan::Text {
                    text: "通常".to_owned(),
                    styles: Vec::new(),
                },
                OcrSpan::Text {
                    text: "強調".to_owned(),
                    styles: vec![TextStyle::Bold, TextStyle::Italic],
                },
                OcrSpan::Text {
                    text: "通常".to_owned(),
                    styles: Vec::new(),
                },
            ],
        };
        assert_eq!(render_srt_line(&line), "通常<b><i>強調</i></b>通常");
    }

    #[test]
    fn flattens_ruby_and_baseline_styles_with_explicit_warnings() {
        let cue = ExportCue {
            id: Uuid::new_v4(),
            index: 7,
            start_ms: 1_000,
            end_ms: 2_000,
            position: SubtitlePosition::BottomCenter,
            geometry: CueGeometry {
                canvas_width: 1920,
                canvas_height: 1080,
                x: 700,
                y: 850,
                width: 520,
                height: 80,
                image_width: 552,
                image_height: 112,
                forced: false,
                inferred_end: false,
            },
            review_status: ReviewStatus::Unreviewed,
            image_sha256: "abc".to_owned(),
            subtitle: OcrDocument {
                prompt_version: "test".to_owned(),
                provider: "test".to_owned(),
                model: "test".to_owned(),
                language: "jpn".to_owned(),
                unreadable: false,
                lines: vec![OcrLine {
                    text: "物語".to_owned(),
                    spans: vec![OcrSpan::Ruby {
                        base: "物語".to_owned(),
                        annotations: vec![rosettacue_domain::RubyAnnotation {
                            text: "ものがたり".to_owned(),
                            position: rosettacue_domain::RubyPosition::Over,
                        }],
                        styles: vec![TextStyle::Superscript],
                    }],
                }],
                normalizations: Vec::new(),
            },
        };

        let (rendered, warnings) = render_srt(&[cue]);
        assert!(rendered.contains("物語"));
        assert!(!rendered.contains("ものがたり"));
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("baseline styles"))
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("ruby annotations"))
        );
    }
}
