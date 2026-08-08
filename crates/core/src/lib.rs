use std::collections::{HashMap, HashSet};
use std::path::Path;

mod layout_survey;

use layout_survey::SurveyBuilder;
pub use layout_survey::{CueLayoutBlock, CueLayoutSummary, LayoutSurvey, LayoutSurveyError};
pub use rosettacue_bluray::configure_media_tools_directory;
pub use rosettacue_bluray::{MediaToolDiagnostic, MediaToolOrigin};
use rosettacue_domain::{
    BlurayDiscInfo, CueEditDocument, CueGeometry, CueRecognition, CueReviewDecision, CueRevision,
    JobKind, JobProgress, JobStatus, OcrSpan, OcrStatus, PgsTrackMetadata, ProjectJob,
    ProjectMetadata, ProjectSettings, ProjectSource, ProjectStatistics, ProperNounMapping,
    ReviewStatus, SourceMetadata, SubtitleCue, SubtitleTrack, TrackMetadata,
};
pub use rosettacue_export::{
    ExportArtifact, ExportFormat, ExportOptions, ExportResult, ExportScope,
};
use rosettacue_ocr::PROMPT_VERSION;
pub use rosettacue_ocr::{
    LayoutTuning, LlmProvider, LmStudioConfig, LmStudioModel, OcrPipelineConfig, ProviderConfig,
    ProviderDiagnostic, ProviderSpec, ReasoningEffort,
};
use rosettacue_ocr::{OcrBackend, OcrRequest, ProviderOcrBackend};
use rosettacue_project::{ProjectError, ProjectStore};
use rosettacue_translation::{SubtitleTranslator, TranslationRequest};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackendInfo {
    pub name: String,
    pub version: String,
    pub project_schema_version: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectOverview {
    pub path: String,
    pub metadata: ProjectMetadata,
    pub statistics: ProjectStatistics,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceImportResult {
    pub source: ProjectSource,
    pub project: ProjectOverview,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectDocument {
    pub project: ProjectOverview,
    pub sources: Vec<ProjectSource>,
    pub tracks: Vec<SubtitleTrack>,
    pub cues: Vec<SubtitleCue>,
    pub recognitions: Vec<CueRecognition>,
    pub revisions: Vec<CueRevision>,
    pub revision_counts: HashMap<Uuid, u64>,
    pub review_decisions: Vec<CueReviewDecision>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewSaveResult {
    pub decision: CueReviewDecision,
    pub project: ProjectOverview,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PgsExtractionProgress {
    pub phase: String,
    pub current: u32,
    pub estimated_total: Option<u32>,
    pub cue: Option<SubtitleCue>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PgsExtractionResult {
    pub track: SubtitleTrack,
    pub cue_count: u32,
    pub project: ProjectOverview,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OcrProgress {
    pub phase: String,
    pub current: u32,
    pub total: u32,
    pub cue_id: Option<Uuid>,
    pub cue_index: Option<u32>,
    pub recognition: Option<CueRecognition>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OcrJobResult {
    pub job_id: Option<Uuid>,
    pub processed: u32,
    pub project: ProjectOverview,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranslationProgress {
    pub phase: String,
    pub current: u32,
    pub total: u32,
    pub cue_id: Option<Uuid>,
    pub cue_index: Option<u32>,
    pub revision: Option<CueRevision>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranslationJobResult {
    pub job_id: Option<Uuid>,
    pub processed: u32,
    pub project: ProjectOverview,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistentOcrRequest {
    cue_ids: Vec<Uuid>,
    language: String,
    overwrite: bool,
    config: OcrPipelineConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistentTranslationRequest {
    cue_ids: Vec<Uuid>,
    target_language: String,
    overwrite: bool,
    #[serde(default)]
    proper_nouns: Vec<ProperNounMapping>,
    config: ProviderConfig,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Application;

impl Application {
    #[must_use]
    pub fn backend_info(self) -> BackendInfo {
        BackendInfo {
            name: "RosettaCue Core".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            project_schema_version: rosettacue_domain::PROJECT_SCHEMA_VERSION,
        }
    }

    /// Reports bundled, configured, and system media-tool availability.
    #[must_use]
    pub fn media_tool_diagnostics(self) -> Vec<MediaToolDiagnostic> {
        rosettacue_bluray::diagnose_media_tools()
    }

    /// Creates a project and returns its canonical metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the project store cannot be created or initialized.
    pub fn create_project(
        self,
        path: impl AsRef<Path>,
        name: &str,
    ) -> Result<ProjectOverview, ProjectError> {
        let store = ProjectStore::create(path, name)?;
        Self::project_overview(&store)
    }

    /// Clones a project package under a new name and switches it to a new
    /// project identity while preserving all subtitle data and assets.
    ///
    /// # Errors
    ///
    /// Returns an error when the name is invalid or the project package cannot
    /// be copied and reopened.
    pub fn save_project_as(
        self,
        project_path: impl AsRef<Path>,
        parent: impl AsRef<Path>,
        name: &str,
    ) -> Result<ProjectOverview, ProjectCloneError> {
        let name = name.trim();
        if name.is_empty()
            || name
                .chars()
                .any(|character| character.is_control() || "\\/:*?\"<>|".contains(character))
        {
            return Err(ProjectCloneError::InvalidName);
        }
        let destination = parent.as_ref().join(format!("{name}.rosettacue"));
        let store = ProjectStore::clone_as(project_path, destination, name)?;
        Ok(Self::project_overview(&store)?)
    }

    /// Opens a project and returns its canonical metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the project cannot be opened or its metadata is invalid.
    pub fn project_info(self, path: impl AsRef<Path>) -> Result<ProjectOverview, ProjectError> {
        let store = ProjectStore::open(path)?;
        Self::project_overview(&store)
    }

    /// Opens the editable contents of a project.
    ///
    /// # Errors
    ///
    /// Returns an error when project records cannot be read.
    pub fn project_document(self, path: impl AsRef<Path>) -> Result<ProjectDocument, ProjectError> {
        let store = ProjectStore::open(path)?;
        Ok(ProjectDocument {
            project: Self::project_overview(&store)?,
            sources: store.sources()?,
            tracks: store.tracks()?,
            cues: store.cues()?,
            recognitions: store.recognitions()?,
            revisions: store.revisions()?,
            revision_counts: store.revision_counts()?,
            review_decisions: store.review_decisions()?,
        })
    }

    /// Validates and persists settings owned by one project package.
    ///
    /// # Errors
    ///
    /// Returns an error when a language or proper-noun mapping is invalid, or
    /// when the project metadata cannot be updated.
    pub fn update_project_settings(
        self,
        project_path: impl AsRef<Path>,
        settings: &ProjectSettings,
    ) -> Result<ProjectOverview, ProjectSettingsError> {
        let settings = normalize_project_settings(settings)?;
        let store = ProjectStore::open(project_path)?;
        store.update_settings(&settings)?;
        Ok(Self::project_overview(&store)?)
    }

    /// Writes the canonical structured subtitle document and requested
    /// derivative formats for one subtitle track.
    ///
    /// # Errors
    ///
    /// Returns an error when the project cannot be read, the track selection is
    /// ambiguous, the selected scope has no recognized cues, or writing fails.
    pub fn export_subtitles(
        self,
        project_path: impl AsRef<Path>,
        options: &ExportOptions,
    ) -> Result<ExportResult, SubtitleExportError> {
        let store = ProjectStore::open(project_path)?;
        let result = rosettacue_export::export_subtitles(
            &store.metadata()?,
            &store.tracks()?,
            &store.cues()?,
            &store.revisions()?,
            options,
        )?;
        for artifact in &result.artifacts {
            store.record_export(
                artifact.format.extension(),
                &artifact.path,
                &serde_json::json!({
                    "track_id": result.track_id,
                    "scope": options.scope,
                    "cue_count": artifact.cue_count,
                    "warnings": artifact.warnings,
                }),
            )?;
        }
        Ok(result)
    }

    /// Saves a validated human edit without replacing the immutable OCR result.
    ///
    /// # Errors
    ///
    /// Returns an error when the cue is missing, the edit is structurally
    /// invalid, or project persistence fails.
    pub fn save_cue_edit(
        self,
        project_path: impl AsRef<Path>,
        cue_id: Uuid,
        document: &CueEditDocument,
    ) -> Result<CueRevision, CueEditError> {
        validate_cue_edit(document)?;
        let store = ProjectStore::open(project_path)?;
        if !store.cues()?.iter().any(|cue| cue.id == cue_id) {
            return Err(CueEditError::CueNotFound);
        }
        Ok(store.save_cue_revision(cue_id, document)?)
    }

    /// Creates a human revision matching the original OCR text and cue metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the cue or OCR result is missing, or persistence fails.
    pub fn restore_cue_edit(
        self,
        project_path: impl AsRef<Path>,
        cue_id: Uuid,
    ) -> Result<CueRevision, CueEditError> {
        let store = ProjectStore::open(project_path)?;
        let cue = store
            .cues()?
            .into_iter()
            .find(|cue| cue.id == cue_id)
            .ok_or(CueEditError::CueNotFound)?;
        let recognition = store
            .recognitions()?
            .into_iter()
            .find(|recognition| recognition.cue_id == cue_id)
            .ok_or(CueEditError::RecognitionNotFound)?;
        let document = CueEditDocument {
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            subtitle: recognition.document,
        };
        Ok(store.save_cue_revision(cue_id, &document)?)
    }

    /// Returns the immutable revision timeline for a Cue, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the project, Cue, or revision data is invalid.
    pub fn cue_revision_history(
        self,
        project_path: impl AsRef<Path>,
        cue_id: Uuid,
    ) -> Result<Vec<CueRevision>, ProjectError> {
        ProjectStore::open(project_path)?.cue_revision_history(cue_id)
    }

    /// Restores a historical snapshot by appending a new human revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision does not belong to the Cue or is invalid.
    pub fn restore_cue_revision(
        self,
        project_path: impl AsRef<Path>,
        cue_id: Uuid,
        revision_id: Uuid,
    ) -> Result<CueRevision, CueEditError> {
        let store = ProjectStore::open(project_path)?;
        let revision = store
            .cue_revision_history(cue_id)?
            .into_iter()
            .find(|revision| revision.id == revision_id)
            .ok_or(CueEditError::RevisionNotFound)?;
        validate_cue_edit(&revision.document)?;
        Ok(store.save_cue_revision(cue_id, &revision.document)?)
    }

    /// Deletes one historical Cue revision and returns the remaining timeline.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision is missing, it is the only remaining
    /// revision, or the project transaction fails.
    pub fn delete_cue_revision(
        self,
        project_path: impl AsRef<Path>,
        cue_id: Uuid,
        revision_id: Uuid,
    ) -> Result<Vec<CueRevision>, ProjectError> {
        let store = ProjectStore::open(project_path)?;
        store.delete_cue_revision(cue_id, revision_id)?;
        store.cue_revision_history(cue_id)
    }

    /// Records a human review decision for the latest effective cue revision.
    ///
    /// # Errors
    ///
    /// Returns an error for an excessive note, a missing revision, or project
    /// persistence failure.
    pub fn review_cue(
        self,
        project_path: impl AsRef<Path>,
        cue_id: Uuid,
        status: ReviewStatus,
        note: &str,
    ) -> Result<ReviewSaveResult, CueEditError> {
        if note.chars().count() > 2_000 {
            return Err(CueEditError::Invalid(
                "a review note cannot exceed 2,000 characters".to_owned(),
            ));
        }
        let store = ProjectStore::open(project_path)?;
        if !store
            .revisions()?
            .iter()
            .any(|revision| revision.cue_id == cue_id)
        {
            return Err(CueEditError::RevisionNotFound);
        }
        let decision = store.save_review_decision(cue_id, status, note.trim())?;
        Ok(ReviewSaveResult {
            decision,
            project: Self::project_overview(&store)?,
        })
    }

    /// Inspects a Blu-ray backup without modifying a project.
    ///
    /// # Errors
    ///
    /// Returns an error when the source is not a valid Blu-ray backup or
    /// libbluray's title inspection tool is unavailable.
    pub fn inspect_bluray_source(
        self,
        path: impl AsRef<Path>,
    ) -> Result<BlurayDiscInfo, rosettacue_bluray::BlurayError> {
        rosettacue_bluray::inspect_disc(path)
    }

    /// Inspects and adds a Blu-ray source to an existing project.
    ///
    /// # Errors
    ///
    /// Returns an error when source inspection or project persistence fails.
    pub fn attach_bluray_source(
        self,
        project_path: impl AsRef<Path>,
        source_path: impl AsRef<Path>,
    ) -> Result<SourceImportResult, SourceImportError> {
        let disc = self.inspect_bluray_source(source_path)?;
        let source = ProjectSource::from_bluray(disc);
        let store = ProjectStore::open(project_path)?;
        store.add_source(&source)?;
        Ok(SourceImportResult {
            source,
            project: Self::project_overview(&store)?,
        })
    }

    /// Extracts, decodes and persists one PGS track while publishing progress.
    ///
    /// # Errors
    ///
    /// Returns an error when the source selection, media tools, PGS stream, or
    /// project persistence fails.
    pub fn extract_pgs_track(
        self,
        project_path: impl AsRef<Path>,
        source_id: Uuid,
        title_index: u32,
        stream_index: u32,
        mut progress: impl FnMut(PgsExtractionProgress),
    ) -> Result<PgsExtractionResult, PgsExtractionError> {
        let store = ProjectStore::open(project_path)?;
        let source = store
            .sources()?
            .into_iter()
            .find(|source| source.id == source_id)
            .ok_or(PgsExtractionError::SourceNotFound(source_id))?;
        let SourceMetadata::Bluray(disc) = &source.metadata;
        let title = disc
            .titles
            .iter()
            .find(|title| title.index == title_index)
            .cloned()
            .ok_or(PgsExtractionError::TitleNotFound(title_index))?;
        if stream_index >= title.pgs_tracks {
            return Err(PgsExtractionError::TrackNotFound(stream_index));
        }
        if store.tracks()?.iter().any(|track| {
            track.source_id == source_id
                && track.stream_index == stream_index
                && matches!(
                    &track.metadata,
                    TrackMetadata::Pgs(metadata) if metadata.title_index == title_index
                )
        }) {
            return Err(PgsExtractionError::AlreadyExtracted);
        }

        let track_id = Uuid::new_v4();
        let sup_relative = format!("assets/tracks/{track_id}/source.sup");
        let sup_path = store.root().join(&sup_relative);
        progress(PgsExtractionProgress::phase("demuxing"));
        rosettacue_bluray::demux_pgs_track(&source.path, &title, stream_index, &sup_path)?;

        let track = SubtitleTrack {
            id: track_id,
            source_id,
            stream_index,
            language: usize::try_from(stream_index)
                .ok()
                .and_then(|index| title.pgs_languages.get(index))
                .cloned(),
            codec: "hdmv_pgs_subtitle".to_owned(),
            metadata: TrackMetadata::Pgs(PgsTrackMetadata {
                title_index,
                playlist: title.playlist.clone(),
                sup_path: sup_relative,
            }),
        };
        store.add_track(&track)?;
        let extraction = decode_track(
            &store,
            &track,
            &sup_path,
            title.duration_seconds,
            &mut progress,
        );
        let cue_count = match extraction {
            Ok(count) => count,
            Err(error) => {
                let _ = store.remove_track(track_id);
                return Err(error);
            }
        };
        progress(PgsExtractionProgress {
            phase: "completed".to_owned(),
            current: cue_count,
            estimated_total: Some(cue_count),
            cue: None,
        });
        Ok(PgsExtractionResult {
            track,
            cue_count,
            project: Self::project_overview(&store)?,
        })
    }

    /// Reads a cue image while confining access to the project's cue directory.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid paths or unreadable images.
    pub fn cue_image(
        self,
        project_path: impl AsRef<Path>,
        image_path: impl AsRef<Path>,
    ) -> Result<Vec<u8>, CueImageError> {
        let store = ProjectStore::open(project_path)?;
        Ok(std::fs::read(confined_cue_path(&store, image_path)?)?)
    }

    /// Analyzes every cue bitmap and reports how the project's layouts break down.
    ///
    /// No provider is contacted: this is the deterministic half of recognition,
    /// run on its own to size up a track before paying to recognize it.
    ///
    /// # Errors
    ///
    /// Returns an error when the project cannot be opened or the language has
    /// no preset. A cue whose bitmap cannot be read is counted and reported,
    /// not raised.
    pub fn survey_cue_layout(
        self,
        project_path: impl AsRef<Path>,
        language: &str,
        tuning: rosettacue_ocr::LayoutTuning,
    ) -> Result<LayoutSurvey, LayoutSurveyError> {
        let store = ProjectStore::open(project_path)?;
        let options = rosettacue_ocr::layout_options(language, tuning)?;
        let mut builder = SurveyBuilder::default();
        for cue in store.cues()? {
            match read_cue_layout(&store, &cue, &options) {
                Ok(layout) => builder.add(&cue, &layout),
                Err(error) => builder.add_failure(&cue, &error),
            }
        }
        Ok(builder.finish())
    }

    /// Lists models currently visible to LM Studio's local server.
    ///
    /// # Errors
    ///
    /// Returns an error when the server is unavailable or returns invalid data.
    pub fn lmstudio_models(
        self,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<LmStudioModel>, rosettacue_ocr::OcrError> {
        rosettacue_ocr::list_lmstudio_models(base_url, api_key)
    }

    /// Lists models visible to any supported LLM provider.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider endpoint is unavailable or malformed.
    pub fn provider_models(
        self,
        provider: LlmProvider,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<LmStudioModel>, rosettacue_ocr::OcrError> {
        rosettacue_ocr::list_provider_models(provider, base_url, api_key)
    }

    #[must_use]
    pub fn diagnose_provider(
        self,
        provider: LlmProvider,
        base_url: &str,
        api_key: Option<&str>,
    ) -> ProviderDiagnostic {
        rosettacue_ocr::diagnose_provider(provider, base_url, api_key)
    }

    /// Recognizes selected cues sequentially through LM Studio.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration, project persistence, image access,
    /// or the OCR provider fails.
    #[allow(clippy::too_many_arguments)]
    pub fn recognize_lmstudio(
        self,
        project_path: impl AsRef<Path>,
        cue_ids: Option<Vec<Uuid>>,
        language: &str,
        overwrite: bool,
        config: &LmStudioConfig,
        mut should_continue: impl FnMut() -> bool,
        mut progress: impl FnMut(OcrProgress),
    ) -> Result<OcrJobResult, OcrJobError> {
        self.recognize_ocr(
            project_path,
            cue_ids,
            language,
            overwrite,
            &OcrPipelineConfig::single(config.clone()),
            &mut should_continue,
            &mut progress,
        )
    }

    /// Recognizes selected cues with independently configurable text, optional ruby, and style models.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration, project persistence, image access,
    /// or any configured LLM provider fails.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn recognize_ocr(
        self,
        project_path: impl AsRef<Path>,
        cue_ids: Option<Vec<Uuid>>,
        language: &str,
        overwrite: bool,
        config: &OcrPipelineConfig,
        mut should_continue: impl FnMut() -> bool,
        mut progress: impl FnMut(OcrProgress),
    ) -> Result<OcrJobResult, OcrJobError> {
        let store = ProjectStore::open(project_path)?;
        let selected = select_ocr_cues(&store, cue_ids, overwrite)?;
        let total = u32::try_from(selected.len()).map_err(|_| OcrJobError::TooManyCues)?;
        if total == 0 {
            return Ok(OcrJobResult {
                job_id: None,
                processed: 0,
                project: Self::project_overview(&store)?,
            });
        }
        let backend = ProviderOcrBackend::with_pipeline(config.clone())?;
        let stored_config = config.redacted();
        let run_id = store.start_ocr_run(
            config.recognition.provider.as_str(),
            &config.recognition.model,
            PROMPT_VERSION,
            language,
            &serde_json::to_value(stored_config)?,
        )?;
        let job = store.enqueue_job(
            JobKind::Ocr,
            &serde_json::to_value(PersistentOcrRequest {
                cue_ids: selected.iter().map(|cue| cue.id).collect(),
                language: language.to_owned(),
                overwrite,
                config: config.redacted(),
            })?,
            &JobProgress {
                phase: "queued".to_owned(),
                current: 0,
                total: Some(total),
                cue_id: None,
                cue_index: None,
                completed_cue_ids: Vec::new(),
            },
        )?;
        store.start_job(job.id)?;
        let mut processed = 0_u32;
        let mut completed_cue_ids = Vec::new();
        for (offset, cue) in selected.iter().enumerate() {
            if !should_continue() {
                store.interrupt_job(job.id)?;
                progress(OcrProgress {
                    phase: "stopped".to_owned(),
                    current: processed,
                    total,
                    cue_id: None,
                    cue_index: None,
                    recognition: None,
                    error: None,
                });
                return Ok(OcrJobResult {
                    job_id: Some(job.id),
                    processed,
                    project: Self::project_overview(&store)?,
                });
            }
            let current = u32::try_from(offset + 1).map_err(|_| OcrJobError::TooManyCues)?;
            store.mark_cue_ocr_running(cue.id)?;
            store.update_job_progress(
                job.id,
                &JobProgress {
                    phase: "running".to_owned(),
                    current,
                    total: Some(total),
                    cue_id: Some(cue.id),
                    cue_index: Some(cue.cue_index),
                    completed_cue_ids: completed_cue_ids.clone(),
                },
            )?;
            progress(OcrProgress::running(current, total, cue));
            let image_path = confined_cue_path(&store, &cue.image_path)?;
            let request = OcrRequest {
                cue_id: cue.id,
                cue_index: cue.cue_index,
                image_path,
                image_sha256: cue.image_sha256.clone(),
                language: language.to_owned(),
                geometry: cue.geometry.clone(),
            };
            match backend.recognize(&request) {
                Ok(result) => {
                    let recognition = store.save_ocr_success(
                        cue.id,
                        run_id,
                        &result.raw_response,
                        &result.document,
                        &result.issues,
                        result.elapsed_ms,
                    )?;
                    progress(OcrProgress {
                        phase: "cue-complete".to_owned(),
                        current,
                        total,
                        cue_id: Some(cue.id),
                        cue_index: Some(cue.cue_index),
                        recognition: Some(recognition),
                        error: None,
                    });
                    processed = processed.saturating_add(1);
                    completed_cue_ids.push(cue.id);
                    store.update_job_progress(
                        job.id,
                        &JobProgress {
                            phase: "cue-complete".to_owned(),
                            current,
                            total: Some(total),
                            cue_id: Some(cue.id),
                            cue_index: Some(cue.cue_index),
                            completed_cue_ids: completed_cue_ids.clone(),
                        },
                    )?;
                }
                Err(error) => {
                    store.save_ocr_failure(cue.id, run_id, &error.to_string())?;
                    progress(OcrProgress {
                        phase: "failed".to_owned(),
                        current,
                        total,
                        cue_id: Some(cue.id),
                        cue_index: Some(cue.cue_index),
                        recognition: None,
                        error: Some(error.to_string()),
                    });
                    store.fail_job(job.id, &error.to_string())?;
                    return Err(OcrJobError::Backend(error));
                }
            }
        }
        progress(OcrProgress {
            phase: "completed".to_owned(),
            current: total,
            total,
            cue_id: None,
            cue_index: None,
            recognition: None,
            error: None,
        });
        store.complete_job(job.id, &serde_json::json!({ "processed": processed }))?;
        Ok(OcrJobResult {
            job_id: Some(job.id),
            processed,
            project: Self::project_overview(&store)?,
        })
    }

    /// Translates selected or pending cues into new translation-authored revisions.
    ///
    /// # Errors
    ///
    /// Returns an error when cue selection, provider inference, validation, or persistence fails.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn translate_cues(
        self,
        project_path: impl AsRef<Path>,
        cue_ids: Option<Vec<Uuid>>,
        target_language: &str,
        overwrite: bool,
        config: &ProviderConfig,
        proper_nouns: Option<&[ProperNounMapping]>,
        mut should_continue: impl FnMut() -> bool,
        mut progress: impl FnMut(TranslationProgress),
    ) -> Result<TranslationJobResult, TranslationJobError> {
        let store = ProjectStore::open(project_path)?;
        let project_settings = store.metadata()?.settings;
        let proper_nouns = proper_nouns.unwrap_or(&project_settings.proper_nouns);
        let cues = store.cues()?;
        let requested = cue_ids.map(|ids| ids.into_iter().collect::<HashSet<_>>());
        if let Some(requested) = &requested
            && !requested
                .iter()
                .all(|id| cues.iter().any(|cue| cue.id == *id))
        {
            return Err(TranslationJobError::CueNotFound);
        }
        let sources = store.translation_sources()?;
        let sources_by_cue = sources
            .into_iter()
            .map(|revision| (revision.cue_id, revision))
            .collect::<HashMap<_, _>>();
        let effective_by_cue = store
            .revisions()?
            .into_iter()
            .map(|revision| (revision.cue_id, revision))
            .collect::<HashMap<_, _>>();
        let selected = cues
            .iter()
            .filter(|cue| requested.as_ref().is_none_or(|ids| ids.contains(&cue.id)))
            .filter_map(|cue| {
                let source = sources_by_cue.get(&cue.id)?;
                if !overwrite
                    && effective_by_cue.get(&cue.id).is_some_and(|revision| {
                        revision.author == rosettacue_domain::RevisionAuthor::Translation
                    })
                {
                    return None;
                }
                Some((cue, source))
            })
            .collect::<Vec<_>>();
        let total = u32::try_from(selected.len()).map_err(|_| TranslationJobError::TooManyCues)?;
        if total == 0 {
            return Ok(TranslationJobResult {
                job_id: None,
                processed: 0,
                project: Self::project_overview(&store)?,
            });
        }
        let translator = SubtitleTranslator::new(config.clone())?;
        let job = store.enqueue_job(
            JobKind::Translation,
            &serde_json::to_value(PersistentTranslationRequest {
                cue_ids: selected.iter().map(|(cue, _)| cue.id).collect(),
                target_language: target_language.to_owned(),
                overwrite,
                proper_nouns: proper_nouns.to_vec(),
                config: config.redacted(),
            })?,
            &JobProgress {
                phase: "queued".to_owned(),
                current: 0,
                total: Some(total),
                cue_id: None,
                cue_index: None,
                completed_cue_ids: Vec::new(),
            },
        )?;
        store.start_job(job.id)?;
        let mut processed = 0_u32;
        let mut completed_cue_ids = Vec::new();
        for (offset, (cue, source)) in selected.iter().enumerate() {
            if !should_continue() {
                store.interrupt_job(job.id)?;
                progress(TranslationProgress {
                    phase: "stopped".to_owned(),
                    current: processed,
                    total,
                    cue_id: None,
                    cue_index: None,
                    revision: None,
                    error: None,
                });
                break;
            }
            let current =
                u32::try_from(offset + 1).map_err(|_| TranslationJobError::TooManyCues)?;
            store.update_job_progress(
                job.id,
                &JobProgress {
                    phase: "running".to_owned(),
                    current,
                    total: Some(total),
                    cue_id: Some(cue.id),
                    cue_index: Some(cue.cue_index),
                    completed_cue_ids: completed_cue_ids.clone(),
                },
            )?;
            progress(TranslationProgress {
                phase: "running".to_owned(),
                current,
                total,
                cue_id: Some(cue.id),
                cue_index: Some(cue.cue_index),
                revision: None,
                error: None,
            });
            let cue_offset = cues
                .iter()
                .position(|item| item.id == cue.id)
                .unwrap_or_default();
            let previous = cues[..cue_offset]
                .iter()
                .rev()
                .find(|item| item.track_id == cue.track_id)
                .and_then(|item| sources_by_cue.get(&item.id))
                .map(|revision| revision.document.subtitle.plain_text());
            let next = cues[cue_offset + 1..]
                .iter()
                .find(|item| item.track_id == cue.track_id)
                .and_then(|item| sources_by_cue.get(&item.id))
                .map(|revision| revision.document.subtitle.plain_text());
            match translator.translate(&TranslationRequest {
                document: &source.document,
                source_language: &source.document.subtitle.language,
                target_language,
                previous_context: previous.as_deref(),
                next_context: next.as_deref(),
                proper_nouns,
            }) {
                Ok(output) => {
                    let revision = store.save_translation_revision(cue.id, &output.document)?;
                    progress(TranslationProgress {
                        phase: "cue-complete".to_owned(),
                        current,
                        total,
                        cue_id: Some(cue.id),
                        cue_index: Some(cue.cue_index),
                        revision: Some(revision),
                        error: None,
                    });
                    processed = processed.saturating_add(1);
                    completed_cue_ids.push(cue.id);
                    store.update_job_progress(
                        job.id,
                        &JobProgress {
                            phase: "cue-complete".to_owned(),
                            current,
                            total: Some(total),
                            cue_id: Some(cue.id),
                            cue_index: Some(cue.cue_index),
                            completed_cue_ids: completed_cue_ids.clone(),
                        },
                    )?;
                }
                Err(error) => {
                    progress(TranslationProgress {
                        phase: "failed".to_owned(),
                        current,
                        total,
                        cue_id: Some(cue.id),
                        cue_index: Some(cue.cue_index),
                        revision: None,
                        error: Some(error.to_string()),
                    });
                    store.fail_job(job.id, &error.to_string())?;
                    return Err(TranslationJobError::Backend(error));
                }
            }
        }
        if processed == total {
            progress(TranslationProgress {
                phase: "completed".to_owned(),
                current: total,
                total,
                cue_id: None,
                cue_index: None,
                revision: None,
                error: None,
            });
            store.complete_job(job.id, &serde_json::json!({ "processed": processed }))?;
        }
        Ok(TranslationJobResult {
            job_id: Some(job.id),
            processed,
            project: Self::project_overview(&store)?,
        })
    }

    /// Lists project jobs and converts stale in-process states to interrupted checkpoints.
    ///
    /// # Errors
    ///
    /// Returns an error when the project or stored job data is invalid.
    pub fn project_jobs(
        self,
        project_path: impl AsRef<Path>,
        recover: bool,
    ) -> Result<Vec<ProjectJob>, ProjectError> {
        let store = ProjectStore::open(project_path)?;
        if recover {
            store.recover_interrupted_jobs()?;
        }
        store.jobs()
    }

    /// Dismisses a recoverable job while retaining its checkpoint as project history.
    ///
    /// # Errors
    ///
    /// Returns an error when the project cannot be opened or the job does not exist.
    pub fn cancel_project_job(
        self,
        project_path: impl AsRef<Path>,
        job_id: Uuid,
    ) -> Result<Vec<ProjectJob>, ProjectError> {
        let store = ProjectStore::open(project_path)?;
        store.cancel_job(job_id, Some(&serde_json::json!({ "dismissed": true })))?;
        store.jobs()
    }

    /// Resumes an interrupted OCR checkpoint using the current session's provider credentials.
    ///
    /// # Errors
    ///
    /// Returns an error when the job is missing, has the wrong kind, or OCR fails.
    pub fn resume_ocr_job(
        self,
        project_path: impl AsRef<Path> + Clone,
        job_id: Uuid,
        config: &OcrPipelineConfig,
        mut should_continue: impl FnMut() -> bool,
        mut progress: impl FnMut(OcrProgress),
    ) -> Result<OcrJobResult, OcrJobError> {
        let store = ProjectStore::open(project_path.clone())?;
        let job = store
            .job(job_id)?
            .ok_or(OcrJobError::PersistentJobNotFound)?;
        if job.kind != JobKind::Ocr
            || !matches!(job.status, JobStatus::Interrupted | JobStatus::Failed)
        {
            return Err(OcrJobError::PersistentJobNotResumable);
        }
        let request = serde_json::from_value::<PersistentOcrRequest>(job.request)?;
        let completed = job
            .progress
            .completed_cue_ids
            .into_iter()
            .collect::<HashSet<_>>();
        let remaining = request
            .cue_ids
            .into_iter()
            .filter(|id| !completed.contains(id))
            .collect::<Vec<_>>();
        let result = self.recognize_ocr(
            project_path,
            Some(remaining),
            &request.language,
            request.overwrite,
            config,
            &mut should_continue,
            &mut progress,
        )?;
        store.cancel_job(
            job_id,
            Some(&serde_json::json!({ "resumed_as": result.job_id })),
        )?;
        Ok(result)
    }

    /// Resumes an interrupted translation checkpoint with the current session credentials.
    ///
    /// # Errors
    ///
    /// Returns an error when the job is missing, has the wrong kind, or translation fails.
    pub fn resume_translation_job(
        self,
        project_path: impl AsRef<Path> + Clone,
        job_id: Uuid,
        config: &ProviderConfig,
        mut should_continue: impl FnMut() -> bool,
        mut progress: impl FnMut(TranslationProgress),
    ) -> Result<TranslationJobResult, TranslationJobError> {
        let store = ProjectStore::open(project_path.clone())?;
        let job = store
            .job(job_id)?
            .ok_or(TranslationJobError::PersistentJobNotFound)?;
        if job.kind != JobKind::Translation
            || !matches!(job.status, JobStatus::Interrupted | JobStatus::Failed)
        {
            return Err(TranslationJobError::PersistentJobNotResumable);
        }
        let request = serde_json::from_value::<PersistentTranslationRequest>(job.request)?;
        let completed = job
            .progress
            .completed_cue_ids
            .into_iter()
            .collect::<HashSet<_>>();
        let remaining = request
            .cue_ids
            .into_iter()
            .filter(|id| !completed.contains(id))
            .collect::<Vec<_>>();
        let result = self.translate_cues(
            project_path,
            Some(remaining),
            &request.target_language,
            request.overwrite,
            config,
            Some(&request.proper_nouns),
            &mut should_continue,
            &mut progress,
        )?;
        store.cancel_job(
            job_id,
            Some(&serde_json::json!({ "resumed_as": result.job_id })),
        )?;
        Ok(result)
    }

    fn project_overview(store: &ProjectStore) -> Result<ProjectOverview, ProjectError> {
        Ok(ProjectOverview {
            path: store.root().to_string_lossy().into_owned(),
            metadata: store.metadata()?,
            statistics: store.statistics()?,
        })
    }
}

impl PgsExtractionProgress {
    fn phase(phase: &str) -> Self {
        Self {
            phase: phase.to_owned(),
            current: 0,
            estimated_total: None,
            cue: None,
        }
    }
}

impl OcrProgress {
    fn running(current: u32, total: u32, cue: &SubtitleCue) -> Self {
        Self {
            phase: "running".to_owned(),
            current,
            total,
            cue_id: Some(cue.id),
            cue_index: Some(cue.cue_index),
            recognition: None,
            error: None,
        }
    }
}

/// Reads and analyzes one cue bitmap, keeping every per-cue failure local.
fn read_cue_layout(
    store: &ProjectStore,
    cue: &SubtitleCue,
    options: &rosettacue_ocr::LayoutOptions,
) -> Result<rosettacue_ocr::CueLayout, Box<dyn std::error::Error>> {
    let image = std::fs::read(confined_cue_path(store, &cue.image_path)?)?;
    Ok(rosettacue_layout::analyze_png(&image, options)?)
}

/// Why a cue image could not be resolved to a path inside the project.
#[derive(Debug, thiserror::Error)]
pub enum ConfinedCuePathError {
    #[error("cue image path is outside the project")]
    OutsideProject,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Resolves a stored cue image path, refusing anything outside the cue directory.
///
/// Every reader of a cue bitmap goes through here, so the confinement rule has
/// one definition rather than one per caller.
fn confined_cue_path(
    store: &ProjectStore,
    image_path: impl AsRef<Path>,
) -> Result<std::path::PathBuf, ConfinedCuePathError> {
    let allowed_root = store.root().join("assets/cues").canonicalize()?;
    let relative = image_path.as_ref();
    if relative.is_absolute() {
        return Err(ConfinedCuePathError::OutsideProject);
    }
    let candidate = store.root().join(relative).canonicalize()?;
    if !candidate.starts_with(allowed_root) {
        return Err(ConfinedCuePathError::OutsideProject);
    }
    Ok(candidate)
}

fn select_ocr_cues(
    store: &ProjectStore,
    cue_ids: Option<Vec<Uuid>>,
    overwrite: bool,
) -> Result<Vec<SubtitleCue>, OcrJobError> {
    let all_cues = store.cues()?;
    let mut selected = if let Some(cue_ids) = cue_ids {
        let requested = cue_ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let selected = all_cues
            .into_iter()
            .filter(|cue| requested.contains(&cue.id))
            .collect::<Vec<_>>();
        if selected.len() != requested.len() {
            return Err(OcrJobError::CueNotFound);
        }
        selected
    } else {
        all_cues
    };
    if !overwrite {
        selected.retain(|cue| cue.ocr_status != OcrStatus::Succeeded);
    }
    Ok(selected)
}

fn normalize_project_settings(
    settings: &ProjectSettings,
) -> Result<ProjectSettings, ProjectSettingsError> {
    fn language(value: &str, label: &str) -> Result<String, ProjectSettingsError> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 32 || value.chars().any(char::is_control) {
            return Err(ProjectSettingsError::Invalid(format!(
                "{label} must contain 1 to 32 visible characters"
            )));
        }
        Ok(value.to_owned())
    }

    if settings.proper_nouns.len() > 500 {
        return Err(ProjectSettingsError::Invalid(
            "a project cannot contain more than 500 proper-noun mappings".to_owned(),
        ));
    }
    let mut seen = HashSet::new();
    let mut proper_nouns = Vec::with_capacity(settings.proper_nouns.len());
    for mapping in &settings.proper_nouns {
        let source = mapping.source.trim();
        let translation = mapping.translation.trim();
        if source.is_empty()
            || translation.is_empty()
            || source.chars().count() > 200
            || translation.chars().count() > 200
            || source.chars().any(char::is_control)
            || translation.chars().any(char::is_control)
        {
            return Err(ProjectSettingsError::Invalid(
                "proper-noun mappings must contain 1 to 200 visible characters on both sides"
                    .to_owned(),
            ));
        }
        if !seen.insert(source.to_owned()) {
            return Err(ProjectSettingsError::Invalid(format!(
                "the proper noun {source:?} is mapped more than once"
            )));
        }
        proper_nouns.push(ProperNounMapping {
            source: source.to_owned(),
            translation: translation.to_owned(),
        });
    }

    Ok(ProjectSettings {
        ocr_language: language(&settings.ocr_language, "OCR language")?,
        target_language: language(&settings.target_language, "translation target language")?,
        proper_nouns,
    })
}

fn validate_cue_edit(document: &CueEditDocument) -> Result<(), CueEditError> {
    if document.start_ms >= document.end_ms {
        return Err(CueEditError::Invalid(
            "the cue end time must be later than its start time".to_owned(),
        ));
    }
    if document.subtitle.blocks.is_empty() {
        return Err(CueEditError::Invalid(
            "the subtitle must contain at least one text block".to_owned(),
        ));
    }
    if document.subtitle.line_count() == 0 {
        return Err(CueEditError::Invalid(
            "the subtitle must contain at least one line".to_owned(),
        ));
    }
    let blocks = document.subtitle.blocks.len();
    for (block_index, block) in document.subtitle.blocks.iter().enumerate() {
        if block.lines.is_empty() {
            return Err(CueEditError::Invalid(format!(
                "subtitle block {} has no lines",
                block_index + 1
            )));
        }
        for (line_index, line) in block.lines.iter().enumerate() {
            validate_line(line, &line_label(blocks, block_index, line_index))?;
        }
    }
    Ok(())
}

/// Names a line the way a person reading the error can find it.
///
/// A cue with one block keeps the plain "line N" it always had; only a cue that
/// actually has blocks pays the cost of saying which.
fn line_label(blocks: usize, block_index: usize, line_index: usize) -> String {
    if blocks > 1 {
        format!("block {} line {}", block_index + 1, line_index + 1)
    } else {
        format!("line {}", line_index + 1)
    }
}

fn validate_line(line: &rosettacue_domain::OcrLine, label: &str) -> Result<(), CueEditError> {
    if line.text.is_empty() || line.text.chars().any(char::is_control) {
        return Err(CueEditError::Invalid(format!(
            "subtitle {label} is empty or contains control characters"
        )));
    }
    if line.spans.is_empty() {
        return Err(CueEditError::Invalid(format!(
            "subtitle {label} has no text spans"
        )));
    }
    let mut composed = String::new();
    for span in &line.spans {
        let styles = span.styles();
        if span
            .color()
            .is_some_and(|color| !is_valid_text_color(color))
        {
            return Err(CueEditError::Invalid(format!(
                "subtitle {label} contains an invalid text color"
            )));
        }
        if styles
            .iter()
            .enumerate()
            .any(|(index, style)| styles[..index].contains(style))
        {
            return Err(CueEditError::Invalid(format!(
                "subtitle {label} contains duplicate text styles"
            )));
        }
        if styles.contains(&rosettacue_domain::TextStyle::Superscript)
            && styles.contains(&rosettacue_domain::TextStyle::Subscript)
        {
            return Err(CueEditError::Invalid(format!(
                "subtitle {label} contains conflicting baseline styles"
            )));
        }
        match span {
            OcrSpan::Text { text, .. } => composed.push_str(text),
            OcrSpan::Ruby {
                base, annotations, ..
            } => {
                if base.is_empty()
                    || annotations.is_empty()
                    || annotations
                        .iter()
                        .any(|annotation| annotation.text.is_empty())
                {
                    return Err(CueEditError::Invalid(format!(
                        "subtitle {label} contains an incomplete ruby span"
                    )));
                }
                composed.push_str(base);
            }
        }
    }
    if composed != line.text {
        return Err(CueEditError::Invalid(format!(
            "subtitle {label} does not match its structured spans"
        )));
    }
    Ok(())
}

fn is_valid_text_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decode_track(
    store: &ProjectStore,
    track: &SubtitleTrack,
    sup_path: &Path,
    duration_seconds: u64,
    progress: &mut impl FnMut(PgsExtractionProgress),
) -> Result<u32, PgsExtractionError> {
    let cue_directory = store.root().join(format!("assets/cues/{}", track.id));
    std::fs::create_dir_all(&cue_directory)?;
    let mut count = 0_u32;
    for decoded in rosettacue_pgs::decode_sup(sup_path, 16)? {
        let image = decoded?;
        count = count
            .checked_add(1)
            .ok_or(PgsExtractionError::TooManyCues)?;
        let image_relative = format!("assets/cues/{}/{count:06}.png", track.id);
        let image_sha256 =
            rosettacue_pgs::write_cue_png(store.root().join(&image_relative), &image)?;
        let geometry = CueGeometry {
            canvas_width: image.canvas_width,
            canvas_height: image.canvas_height,
            x: image.bbox.0,
            y: image.bbox.1,
            width: image.bbox.2,
            height: image.bbox.3,
            image_width: image.width,
            image_height: image.height,
            forced: image.forced,
            inferred_end: image.inferred_end,
        };
        let cue = SubtitleCue {
            id: Uuid::new_v4(),
            track_id: track.id,
            cue_index: count,
            start_ms: image.start_ms(),
            end_ms: image.end_ms(),
            image_path: image_relative,
            image_sha256,
            position: geometry.position(),
            geometry,
            ocr_status: OcrStatus::Pending,
            review_status: ReviewStatus::Unreviewed,
        };
        store.add_cue(&cue)?;
        let elapsed_seconds = cue.end_ms / 1_000;
        let estimated_total = if elapsed_seconds > 0 {
            u32::try_from(
                u64::from(count)
                    .saturating_mul(duration_seconds)
                    .div_ceil(elapsed_seconds),
            )
            .ok()
        } else {
            None
        };
        progress(PgsExtractionProgress {
            phase: "decoding".to_owned(),
            current: count,
            estimated_total,
            cue: Some(cue),
        });
    }
    if count == 0 {
        return Err(PgsExtractionError::NoCues);
    }
    Ok(count)
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectSettingsError {
    #[error("invalid project settings: {0}")]
    Invalid(String),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[derive(Debug, thiserror::Error)]
pub enum SourceImportError {
    #[error(transparent)]
    Bluray(#[from] rosettacue_bluray::BlurayError),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[derive(Debug, thiserror::Error)]
pub enum PgsExtractionError {
    #[error("Blu-ray source {0} was not found in the project")]
    SourceNotFound(Uuid),
    #[error("Blu-ray title {0} was not found")]
    TitleNotFound(u32),
    #[error("PGS track {0} was not found")]
    TrackNotFound(u32),
    #[error("this PGS track has already been extracted")]
    AlreadyExtracted,
    #[error("the PGS stream contains no visible cues")]
    NoCues,
    #[error("the PGS stream contains too many cues")]
    TooManyCues,
    #[error(transparent)]
    Bluray(#[from] rosettacue_bluray::BlurayError),
    #[error(transparent)]
    Pgs(#[from] rosettacue_pgs::PgsError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CueImageError {
    #[error(transparent)]
    CuePath(#[from] ConfinedCuePathError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CueEditError {
    #[error("cue was not found in the project")]
    CueNotFound,
    #[error("the cue has no OCR result to restore")]
    RecognitionNotFound,
    #[error("the cue has no revision to review")]
    RevisionNotFound,
    #[error("invalid cue edit: {0}")]
    Invalid(String),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[derive(Debug, thiserror::Error)]
pub enum SubtitleExportError {
    #[error(transparent)]
    Export(#[from] rosettacue_export::ExportError),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectCloneError {
    #[error("project name is empty or contains a reserved path character")]
    InvalidName,
    #[error(transparent)]
    Project(#[from] ProjectError),
}

#[derive(Debug, thiserror::Error)]
pub enum OcrJobError {
    #[error("one or more selected cues were not found")]
    CueNotFound,
    #[error("too many cues were selected")]
    TooManyCues,
    #[error(transparent)]
    CuePath(#[from] ConfinedCuePathError),
    #[error("persistent OCR job was not found")]
    PersistentJobNotFound,
    #[error("persistent OCR job is not resumable")]
    PersistentJobNotResumable,
    #[error(transparent)]
    Backend(#[from] rosettacue_ocr::OcrError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum TranslationJobError {
    #[error("one or more selected cues were not found")]
    CueNotFound,
    #[error("too many cues were selected")]
    TooManyCues,
    #[error("persistent translation job was not found")]
    PersistentJobNotFound,
    #[error("persistent translation job is not resumable")]
    PersistentJobNotResumable,
    #[error(transparent)]
    Backend(#[from] rosettacue_translation::TranslationError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosettacue_domain::{
        BlockBounds, BlockSource, OcrDocument, OcrLine, RubyAnnotation, RubyPosition,
        SubtitlePosition, TextBlock, TextStyle, WritingMode,
    };

    fn ruby_line() -> OcrLine {
        OcrLine {
            text: "物語".to_owned(),
            spans: vec![OcrSpan::Ruby {
                base: "物語".to_owned(),
                annotations: vec![RubyAnnotation {
                    text: "ものがたり".to_owned(),
                    position: RubyPosition::Over,
                }],
                styles: vec![TextStyle::Italic],
                color: None,
            }],
        }
    }

    fn text_block(position: SubtitlePosition, lines: Vec<OcrLine>) -> TextBlock {
        TextBlock {
            bounds: BlockBounds {
                x: 700,
                y: 850,
                width: 520,
                height: 80,
            },
            writing_mode: WritingMode::HorizontalTb,
            position,
            source: BlockSource::Detected,
            lines,
        }
    }

    fn valid_edit() -> CueEditDocument {
        CueEditDocument {
            start_ms: 100,
            end_ms: 900,
            subtitle: OcrDocument {
                prompt_version: "test".to_owned(),
                provider: "test".to_owned(),
                model: "test".to_owned(),
                language: "jpn".to_owned(),
                unreadable: false,
                blocks: vec![text_block(
                    SubtitlePosition::BottomCenter,
                    vec![ruby_line()],
                )],
                normalizations: Vec::new(),
            },
        }
    }

    #[test]
    fn validates_structured_cue_edits() {
        assert!(validate_cue_edit(&valid_edit()).is_ok());

        let mut mismatched = valid_edit();
        mismatched.subtitle.blocks[0].lines[0].text = "別の文字".to_owned();
        assert!(matches!(
            validate_cue_edit(&mismatched),
            Err(CueEditError::Invalid(_))
        ));

        let mut invalid_timing = valid_edit();
        invalid_timing.end_ms = invalid_timing.start_ms;
        assert!(matches!(
            validate_cue_edit(&invalid_timing),
            Err(CueEditError::Invalid(_))
        ));

        let mut conflicting_baseline = valid_edit();
        conflicting_baseline.subtitle.blocks[0].lines[0].spans[0] = OcrSpan::Text {
            text: "物語".to_owned(),
            styles: vec![TextStyle::Superscript, TextStyle::Subscript],
            color: None,
        };
        assert!(matches!(
            validate_cue_edit(&conflicting_baseline),
            Err(CueEditError::Invalid(message)) if message.contains("conflicting baseline styles")
        ));

        let mut duplicate_style = valid_edit();
        duplicate_style.subtitle.blocks[0].lines[0].spans[0] = OcrSpan::Text {
            text: "物語".to_owned(),
            styles: vec![TextStyle::Italic, TextStyle::Italic],
            color: None,
        };
        assert!(matches!(
            validate_cue_edit(&duplicate_style),
            Err(CueEditError::Invalid(message)) if message.contains("duplicate text styles")
        ));

        let mut invalid_color = valid_edit();
        *invalid_color.subtitle.blocks[0].lines[0].spans[0].color_mut() = Some("tomato".to_owned());
        assert!(matches!(
            validate_cue_edit(&invalid_color),
            Err(CueEditError::Invalid(message)) if message.contains("invalid text color")
        ));
    }

    #[test]
    fn names_the_block_only_once_a_cue_actually_has_blocks() {
        let mut single = valid_edit();
        single.subtitle.blocks[0].lines[0].text = "別の文字".to_owned();
        assert!(matches!(
            validate_cue_edit(&single),
            Err(CueEditError::Invalid(message))
                if message == "subtitle line 1 does not match its structured spans"
        ));

        let mut two = valid_edit();
        two.subtitle.blocks.push(text_block(
            SubtitlePosition::TopRight,
            vec![ruby_line(), ruby_line()],
        ));
        two.subtitle.blocks[1].lines[1].text = "別の文字".to_owned();
        assert!(matches!(
            validate_cue_edit(&two),
            Err(CueEditError::Invalid(message))
                if message == "subtitle block 2 line 2 does not match its structured spans"
        ));
    }

    #[test]
    fn rejects_a_block_with_no_lines() {
        let mut empty = valid_edit();
        empty.subtitle.blocks[0].lines.clear();
        assert!(matches!(
            validate_cue_edit(&empty),
            Err(CueEditError::Invalid(_))
        ));
    }

    #[test]
    fn normalizes_and_validates_project_proper_nouns() {
        let normalized = normalize_project_settings(&ProjectSettings {
            ocr_language: " jpn ".to_owned(),
            target_language: " eng ".to_owned(),
            proper_nouns: vec![ProperNounMapping {
                source: " 綾瀬千早 ".to_owned(),
                translation: " Chihaya Ayase ".to_owned(),
            }],
        })
        .expect("valid settings");
        assert_eq!(normalized.ocr_language, "jpn");
        assert_eq!(normalized.target_language, "eng");
        assert_eq!(normalized.proper_nouns[0].source, "綾瀬千早");

        let duplicate = ProjectSettings {
            proper_nouns: vec![
                ProperNounMapping {
                    source: "千早".to_owned(),
                    translation: "Chihaya".to_owned(),
                },
                ProperNounMapping {
                    source: "千早".to_owned(),
                    translation: "Chihaya".to_owned(),
                },
            ],
            ..ProjectSettings::default()
        };
        assert!(matches!(
            normalize_project_settings(&duplicate),
            Err(ProjectSettingsError::Invalid(message)) if message.contains("mapped more than once")
        ));
    }
}
