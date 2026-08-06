mod error;
mod schema;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rosettacue_diagnostics::{DiagnosticEvent, DiagnosticLevel};
use rosettacue_domain::{
    CueEditDocument, CueGeometry, CueRecognition, CueReviewDecision, CueRevision, JobKind,
    JobProgress, JobStatus, OcrDocument, OcrStatus, ProjectJob, ProjectMetadata, ProjectSettings,
    ProjectSource, ProjectStatistics, ReviewStatus, RevisionAuthor, SourceKind, SourceMetadata,
    SubtitleCue, SubtitleTrack, TrackMetadata,
};
use rusqlite::{Connection, OptionalExtension, params};
use time::OffsetDateTime;
use uuid::Uuid;

pub use error::ProjectError;

const DATABASE_FILENAME: &str = "project.sqlite";

pub struct ProjectStore {
    root: PathBuf,
    connection: Connection,
}

impl ProjectStore {
    /// Creates a new project directory and initializes its database.
    ///
    /// # Errors
    ///
    /// Returns an error when the destination is not empty, a directory cannot be
    /// created, `SQLite` initialization fails, or metadata cannot be serialized.
    pub fn create(root: impl AsRef<Path>, name: &str) -> Result<Self, ProjectError> {
        let root = root.as_ref().to_path_buf();
        if root.exists() && root.read_dir()?.next().is_some() {
            return Err(ProjectError::DirectoryNotEmpty(root));
        }

        fs::create_dir_all(root.join("assets/cues"))?;
        fs::create_dir_all(root.join("assets/thumbnails"))?;
        fs::create_dir_all(root.join("assets/proxy"))?;
        fs::create_dir_all(root.join("cache/ocr"))?;
        fs::create_dir_all(root.join("exports"))?;
        fs::create_dir_all(root.join("logs"))?;

        let connection = Connection::open(root.join(DATABASE_FILENAME))?;
        schema::initialize(&connection)?;
        let metadata = ProjectMetadata::new(name);
        let metadata_json = serde_json::to_string(&metadata)?;
        connection.execute(
            "INSERT INTO project_metadata (id, document) VALUES (1, ?1)",
            params![metadata_json],
        )?;

        project_event(
            "create_project",
            "committed",
            "Project store created.",
            || serde_json::json!({ "path": root, "name": name, "project_id": metadata.id }),
        );

        Ok(Self { root, connection })
    }

    /// Clones a complete project package and assigns new project identity.
    ///
    /// The copy is assembled in a sibling temporary directory and renamed to
    /// the final destination only after its metadata has been updated.
    ///
    /// # Errors
    ///
    /// Returns an error when the source is invalid, the destination exists or
    /// is nested inside the source, a symbolic link is encountered, or copying
    /// and metadata persistence fail.
    pub fn clone_as(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        name: &str,
    ) -> Result<Self, ProjectError> {
        let source_store = Self::open(source)?;
        let source_root = source_store.root.canonicalize()?;
        let original_metadata = source_store.metadata()?;
        drop(source_store);

        let destination = destination.as_ref().to_path_buf();
        if destination.exists() {
            return Err(ProjectError::DestinationExists(destination));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| ProjectError::DestinationInsideSource(destination.clone()))?;
        fs::create_dir_all(parent)?;
        let canonical_parent = parent.canonicalize()?;
        let file_name = destination
            .file_name()
            .ok_or_else(|| ProjectError::DestinationInsideSource(destination.clone()))?;
        let canonical_destination = canonical_parent.join(file_name);
        if canonical_destination.starts_with(&source_root) {
            return Err(ProjectError::DestinationInsideSource(destination));
        }

        let temporary = canonical_parent.join(format!(".rosettacue-copy-{}", Uuid::new_v4()));
        fs::create_dir(&temporary)?;
        let result = (|| {
            copy_directory_contents(&source_root, &temporary)?;
            let copied = Self::open(&temporary)?;
            let metadata = ProjectMetadata::cloned_from(
                &original_metadata,
                name,
                source_root.to_string_lossy(),
            );
            copied.replace_metadata(&metadata)?;
            drop(copied);
            fs::rename(&temporary, &canonical_destination)?;
            Self::open(&canonical_destination)
        })();
        if result.is_err() && temporary.exists() {
            let _ = fs::remove_dir_all(&temporary);
        }
        result
    }

    /// Opens an existing project whose schema exactly matches this build.
    ///
    /// # Errors
    ///
    /// Returns an error when the project database is missing, cannot be opened,
    /// or uses a different schema version.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let root = root.as_ref().to_path_buf();
        let database = root.join(DATABASE_FILENAME);
        if !database.is_file() {
            return Err(ProjectError::NotAProject(root));
        }

        let connection = Connection::open(database)?;
        schema::validate(&connection)?;
        let store = Self { root, connection };
        let metadata = store.metadata()?;
        if metadata.schema_version != rosettacue_domain::PROJECT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchema {
                found: metadata.schema_version,
                expected: rosettacue_domain::PROJECT_SCHEMA_VERSION,
            });
        }
        Ok(store)
    }

    /// Reads the canonical project metadata document.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata is missing, unreadable, or invalid JSON.
    pub fn metadata(&self) -> Result<ProjectMetadata, ProjectError> {
        let document = self
            .connection
            .query_row(
                "SELECT document FROM project_metadata WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(ProjectError::MissingMetadata)?;
        Ok(serde_json::from_str(&document)?)
    }

    fn replace_metadata(&self, metadata: &ProjectMetadata) -> Result<(), ProjectError> {
        let document = serde_json::to_string(metadata)?;
        let updated = self.connection.execute(
            "UPDATE project_metadata SET document = ?1 WHERE id = 1",
            params![document],
        )?;
        if updated != 1 {
            return Err(ProjectError::MissingMetadata);
        }
        project_event(
            "replace_metadata",
            "committed",
            "Project metadata replaced.",
            || serde_json::json!({ "project_id": metadata.id, "name": metadata.name }),
        );
        Ok(())
    }

    /// Replaces settings stored inside this project package.
    ///
    /// # Errors
    ///
    /// Returns an error when project metadata cannot be read, serialized, or updated.
    pub fn update_settings(
        &self,
        settings: &ProjectSettings,
    ) -> Result<ProjectMetadata, ProjectError> {
        let mut metadata = self.metadata()?;
        metadata.settings = settings.clone();
        metadata.updated_at = OffsetDateTime::now_utc();
        self.replace_metadata(&metadata)?;
        project_event(
            "update_settings",
            "committed",
            "Project settings updated.",
            || serde_json::json!({ "project_id": metadata.id, "settings": settings }),
        );
        Ok(metadata)
    }

    /// Returns lightweight counts used by project launchers and workspaces.
    ///
    /// # Errors
    ///
    /// Returns an error when the project database cannot be queried.
    pub fn statistics(&self) -> Result<ProjectStatistics, ProjectError> {
        let counts: (i64, i64, i64, i64, i64) = self
            .connection
            .query_row(
                r"
                SELECT
                    (SELECT COUNT(*) FROM sources),
                    (SELECT COUNT(*) FROM tracks),
                    (SELECT COUNT(*) FROM cues),
                    (SELECT COUNT(*) FROM cues WHERE ocr_status = 'succeeded'),
                    (SELECT COUNT(*) FROM cues WHERE review_status = 'approved')
                ",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .map_err(ProjectError::from)?;
        Ok(ProjectStatistics {
            source_count: u64::try_from(counts.0).map_err(|_| ProjectError::InvalidStatistics)?,
            track_count: u64::try_from(counts.1).map_err(|_| ProjectError::InvalidStatistics)?,
            cue_count: u64::try_from(counts.2).map_err(|_| ProjectError::InvalidStatistics)?,
            ocr_completed_count: u64::try_from(counts.3)
                .map_err(|_| ProjectError::InvalidStatistics)?,
            reviewed_count: u64::try_from(counts.4).map_err(|_| ProjectError::InvalidStatistics)?,
        })
    }

    /// Adds an analyzed source to the project.
    ///
    /// # Errors
    ///
    /// Returns an error when the source metadata cannot be serialized or the
    /// project database rejects the new source.
    pub fn add_source(&self, source: &ProjectSource) -> Result<(), ProjectError> {
        let metadata = serde_json::to_string(&source.metadata)?;
        self.connection.execute(
            r"
            INSERT INTO sources (
                id, kind, display_name, path, fingerprint, created_at, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                source.id.to_string(),
                source.kind.as_str(),
                source.display_name,
                source.path,
                source.fingerprint,
                source.created_at.unix_timestamp().to_string(),
                metadata,
            ],
        )?;
        project_event("add_source", "committed", "Project source added.", || {
            serde_json::json!({
                "source_id": source.id,
                "kind": source.kind.as_str(),
                "display_name": source.display_name,
                "path": source.path
            })
        });
        Ok(())
    }

    /// Returns every imported source in insertion order.
    ///
    /// # Errors
    ///
    /// Returns an error when a database row contains invalid serialized data.
    pub fn sources(&self) -> Result<Vec<ProjectSource>, ProjectError> {
        let mut statement = self.connection.prepare(
            "SELECT id, kind, display_name, path, fingerprint, metadata, created_at \
             FROM sources ORDER BY rowid",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        rows.map(|row| parse_source_row(row?)).collect()
    }

    /// Adds a demultiplexed subtitle track.
    ///
    /// # Errors
    ///
    /// Returns an error when track metadata cannot be serialized or inserted.
    pub fn add_track(&self, track: &SubtitleTrack) -> Result<(), ProjectError> {
        self.connection.execute(
            r"
            INSERT INTO tracks (id, source_id, stream_index, language, codec, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                track.id.to_string(),
                track.source_id.to_string(),
                track.stream_index,
                track.language,
                track.codec,
                serde_json::to_string(&track.metadata)?,
            ],
        )?;
        project_event("add_track", "committed", "Subtitle track added.", || {
            serde_json::json!({
                "track_id": track.id,
                "source_id": track.source_id,
                "stream_index": track.stream_index,
                "language": track.language
            })
        });
        Ok(())
    }

    /// Removes a track and its cues after a failed extraction.
    ///
    /// # Errors
    ///
    /// Returns an error when the database delete fails.
    pub fn remove_track(&self, track_id: Uuid) -> Result<(), ProjectError> {
        self.connection.execute(
            "DELETE FROM tracks WHERE id = ?1",
            params![track_id.to_string()],
        )?;
        project_event(
            "remove_track",
            "committed",
            "Subtitle track removed.",
            || serde_json::json!({ "track_id": track_id }),
        );
        Ok(())
    }

    /// Adds one decoded subtitle cue.
    ///
    /// # Errors
    ///
    /// Returns an error when geometry cannot be serialized or inserted.
    pub fn add_cue(&self, cue: &SubtitleCue) -> Result<(), ProjectError> {
        let start_ms = i64::try_from(cue.start_ms)
            .map_err(|_| ProjectError::InvalidRecord("cue start time".to_owned()))?;
        let end_ms = i64::try_from(cue.end_ms)
            .map_err(|_| ProjectError::InvalidRecord("cue end time".to_owned()))?;
        self.connection.execute(
            r"
            INSERT INTO cues (
                id, track_id, cue_index, start_ms, end_ms, image_path,
                image_sha256, geometry, ocr_status, review_status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                cue.id.to_string(),
                cue.track_id.to_string(),
                cue.cue_index,
                start_ms,
                end_ms,
                cue.image_path,
                cue.image_sha256,
                serde_json::to_string(&cue.geometry)?,
                cue.ocr_status.as_str(),
                cue.review_status.as_str(),
            ],
        )?;
        project_event("add_cue", "committed", "Subtitle cue added.", || {
            serde_json::json!({
                "cue_id": cue.id,
                "track_id": cue.track_id,
                "cue_index": cue.cue_index,
                "start_ms": cue.start_ms,
                "end_ms": cue.end_ms,
                "image_path": cue.image_path,
                "image_sha256": cue.image_sha256
            })
        });
        Ok(())
    }

    /// Returns all subtitle tracks.
    ///
    /// # Errors
    ///
    /// Returns an error when a row is invalid.
    pub fn tracks(&self) -> Result<Vec<SubtitleTrack>, ProjectError> {
        let mut statement = self.connection.prepare(
            "SELECT id, source_id, stream_index, language, codec, metadata \
             FROM tracks ORDER BY rowid",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|row| parse_track_row(row?)).collect()
    }

    /// Returns all cues ordered by track and cue index.
    ///
    /// # Errors
    ///
    /// Returns an error when a row is invalid.
    pub fn cues(&self) -> Result<Vec<SubtitleCue>, ProjectError> {
        let mut statement = self.connection.prepare(
            "SELECT id, track_id, cue_index, start_ms, end_ms, image_path, \
             image_sha256, geometry, ocr_status, review_status \
             FROM cues ORDER BY track_id, cue_index",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?;
        rows.map(|row| parse_cue_row(row?)).collect()
    }

    /// Creates one OCR run shared by all selected cues.
    ///
    /// # Errors
    ///
    /// Returns an error when settings cannot be serialized or inserted.
    pub fn start_ocr_run(
        &self,
        provider: &str,
        model: &str,
        prompt_version: &str,
        language: &str,
        settings: &serde_json::Value,
    ) -> Result<Uuid, ProjectError> {
        let id = Uuid::new_v4();
        self.connection.execute(
            r"
            INSERT INTO ocr_runs (
                id, provider, model, prompt_version, language, settings, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                id.to_string(),
                provider,
                model,
                prompt_version,
                language,
                serde_json::to_string(settings)?,
                OffsetDateTime::now_utc().unix_timestamp().to_string(),
            ],
        )?;
        project_event("start_ocr_run", "committed", "OCR run created.", || {
            serde_json::json!({
                "run_id": id,
                "provider": provider,
                "model": model,
                "prompt_version": prompt_version,
                "language": language
            })
        });
        Ok(id)
    }

    /// Marks a cue as actively being recognized.
    ///
    /// # Errors
    ///
    /// Returns an error when the update fails.
    pub fn mark_cue_ocr_running(&self, cue_id: Uuid) -> Result<(), ProjectError> {
        self.connection.execute(
            "UPDATE cues SET ocr_status = 'running' WHERE id = ?1",
            params![cue_id.to_string()],
        )?;
        Ok(())
    }

    /// Persists the validated OCR document and promotes it to the latest revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the transaction cannot be committed.
    pub fn save_ocr_success(
        &self,
        cue_id: Uuid,
        run_id: Uuid,
        raw_response: &str,
        document: &OcrDocument,
        elapsed_ms: u64,
    ) -> Result<CueRecognition, ProjectError> {
        let timestamp = OffsetDateTime::now_utc().unix_timestamp();
        let now = OffsetDateTime::from_unix_timestamp(timestamp)
            .map_err(|_| ProjectError::InvalidRecord("OCR revision timestamp".to_owned()))?;
        let (start_ms, end_ms, position) = self.cue_edit_base(cue_id)?;
        let document_json = serde_json::to_string(&CueEditDocument {
            start_ms,
            end_ms,
            position,
            subtitle: document.clone(),
        })?;
        let elapsed_ms = i64::try_from(elapsed_ms)
            .map_err(|_| ProjectError::InvalidRecord("OCR elapsed time".to_owned()))?;
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            self.connection.execute(
                r"
                INSERT INTO ocr_attempts (
                    id, cue_id, run_id, attempt_number, status, raw_response,
                    candidate, issues, elapsed_ms, created_at
                ) VALUES (?1, ?2, ?3, 1, 'succeeded', ?4, ?5, '[]', ?6, ?7)
                ",
                params![
                    Uuid::new_v4().to_string(),
                    cue_id.to_string(),
                    run_id.to_string(),
                    raw_response,
                    document_json,
                    elapsed_ms,
                    timestamp.to_string(),
                ],
            )?;
            self.connection.execute(
                "INSERT INTO cue_revisions (id, cue_id, author, document, created_at) \
                 VALUES (?1, ?2, 'ocr', ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    cue_id.to_string(),
                    document_json,
                    timestamp.to_string(),
                ],
            )?;
            self.connection.execute(
                "UPDATE cues SET ocr_status = 'succeeded', review_status = 'unreviewed' WHERE id = ?1",
                params![cue_id.to_string()],
            )?;
            Ok::<_, ProjectError>(())
        })();
        if let Err(error) = result {
            let _ = self.connection.execute_batch("ROLLBACK");
            project_event(
                "save_ocr_success",
                "rolled_back",
                "OCR result transaction rolled back.",
                || serde_json::json!({ "cue_id": cue_id, "run_id": run_id, "error": error.to_string() }),
            );
            return Err(error);
        }
        self.connection.execute_batch("COMMIT")?;
        project_event(
            "save_ocr_success",
            "committed",
            "OCR result and revision saved.",
            || serde_json::json!({ "cue_id": cue_id, "run_id": run_id, "elapsed_ms": elapsed_ms }),
        );
        Ok(CueRecognition {
            cue_id,
            document: document.clone(),
            created_at: now,
        })
    }

    /// Records a failed OCR attempt and makes the cue retryable.
    ///
    /// # Errors
    ///
    /// Returns an error when the database update fails.
    pub fn save_ocr_failure(
        &self,
        cue_id: Uuid,
        run_id: Uuid,
        error: &str,
    ) -> Result<(), ProjectError> {
        let now = OffsetDateTime::now_utc().unix_timestamp().to_string();
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            self.connection.execute(
                r"
                INSERT INTO ocr_attempts (
                    id, cue_id, run_id, attempt_number, status, issues, created_at
                ) VALUES (?1, ?2, ?3, 1, 'failed', ?4, ?5)
                ",
                params![
                    Uuid::new_v4().to_string(),
                    cue_id.to_string(),
                    run_id.to_string(),
                    serde_json::to_string(&vec![error])?,
                    now,
                ],
            )?;
            self.connection.execute(
                "UPDATE cues SET ocr_status = 'failed' WHERE id = ?1",
                params![cue_id.to_string()],
            )?;
            Ok::<_, ProjectError>(())
        })();
        if let Err(error) = result {
            let _ = self.connection.execute_batch("ROLLBACK");
            project_event(
                "save_ocr_failure",
                "rolled_back",
                "OCR failure transaction rolled back.",
                || serde_json::json!({ "cue_id": cue_id, "run_id": run_id, "error": error.to_string() }),
            );
            return Err(error);
        }
        self.connection.execute_batch("COMMIT")?;
        project_event(
            "save_ocr_failure",
            "committed",
            "OCR failure saved.",
            || serde_json::json!({ "cue_id": cue_id, "run_id": run_id, "error": error }),
        );
        Ok(())
    }

    /// Returns the latest OCR-authored revision for every recognized cue.
    ///
    /// # Errors
    ///
    /// Returns an error when a revision is invalid.
    pub fn recognitions(&self) -> Result<Vec<CueRecognition>, ProjectError> {
        let mut statement = self.connection.prepare(
            r"
            SELECT revision.cue_id, revision.document, revision.created_at
            FROM cue_revisions AS revision
            WHERE revision.author = 'ocr'
              AND revision.rowid = (
                SELECT MAX(latest.rowid)
                FROM cue_revisions AS latest
                WHERE latest.cue_id = revision.cue_id AND latest.author = 'ocr'
              )
            ORDER BY revision.rowid
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| parse_recognition_row(row?)).collect()
    }

    /// Returns the latest effective revision for every cue with recognized text.
    ///
    /// # Errors
    ///
    /// Returns an error when revision or cue geometry data is invalid.
    pub fn revisions(&self) -> Result<Vec<CueRevision>, ProjectError> {
        let mut statement = self.connection.prepare(
            r"
            SELECT revision.id, revision.cue_id, revision.author, revision.document,
                   revision.created_at
            FROM cue_revisions AS revision
            JOIN cues AS cue ON cue.id = revision.cue_id
            WHERE revision.rowid = (
                SELECT MAX(latest.rowid)
                FROM cue_revisions AS latest
                WHERE latest.cue_id = revision.cue_id
            )
            ORDER BY cue.track_id, cue.cue_index
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            parse_revision_row(&row)
        })
        .collect()
    }

    /// Returns the number of saved revisions for every cue that has history.
    ///
    /// # Errors
    ///
    /// Returns an error when revision counts or cue identifiers are invalid.
    pub fn revision_counts(&self) -> Result<HashMap<Uuid, u64>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT cue_id, COUNT(*) FROM cue_revisions GROUP BY cue_id")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.map(|row| {
            let (cue_id, count) = row?;
            Ok((
                parse_uuid(&cue_id, "revision count cue id")?,
                u64::try_from(count)
                    .map_err(|_| ProjectError::InvalidRecord("revision count".to_owned()))?,
            ))
        })
        .collect()
    }

    /// Returns every revision for one cue, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the cue is missing or stored revision data is invalid.
    pub fn cue_revision_history(&self, cue_id: Uuid) -> Result<Vec<CueRevision>, ProjectError> {
        self.cue_edit_base(cue_id)?;
        let mut statement = self.connection.prepare(
            r"
            SELECT revision.id, revision.cue_id, revision.author, revision.document,
                   revision.created_at
            FROM cue_revisions AS revision
            JOIN cues AS cue ON cue.id = revision.cue_id
            WHERE revision.cue_id = ?1
            ORDER BY revision.rowid DESC
            ",
        )?;
        let rows = statement.query_map(params![cue_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| parse_revision_row(&row?)).collect()
    }

    /// Deletes one historical revision while keeping at least one effective revision.
    ///
    /// Deleting a revision invalidates the Cue's review decision. If the current
    /// revision is deleted, the preceding revision becomes effective.
    ///
    /// # Errors
    ///
    /// Returns an error when the Cue or revision is missing, it is the Cue's only
    /// remaining revision, or the transaction fails.
    pub fn delete_cue_revision(&self, cue_id: Uuid, revision_id: Uuid) -> Result<(), ProjectError> {
        self.cue_edit_base(cue_id)?;
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let revision_exists: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM cue_revisions WHERE id = ?1 AND cue_id = ?2)",
                params![revision_id.to_string(), cue_id.to_string()],
                |row| row.get(0),
            )?;
            if !revision_exists {
                return Err(ProjectError::RevisionNotFound);
            }
            let revision_count: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM cue_revisions WHERE cue_id = ?1",
                params![cue_id.to_string()],
                |row| row.get(0),
            )?;
            if revision_count <= 1 {
                return Err(ProjectError::LastRevision);
            }
            let deleted = self.connection.execute(
                "DELETE FROM cue_revisions WHERE id = ?1 AND cue_id = ?2",
                params![revision_id.to_string(), cue_id.to_string()],
            )?;
            debug_assert_eq!(deleted, 1);
            self.connection.execute(
                "UPDATE cues SET review_status = 'unreviewed' WHERE id = ?1",
                params![cue_id.to_string()],
            )?;
            Ok::<_, ProjectError>(())
        })();
        if let Err(error) = result {
            let _ = self.connection.execute_batch("ROLLBACK");
            project_event(
                "delete_cue_revision",
                "rolled_back",
                "Revision deletion rolled back.",
                || serde_json::json!({ "cue_id": cue_id, "revision_id": revision_id, "error": error.to_string() }),
            );
            return Err(error);
        }
        self.connection.execute_batch("COMMIT")?;
        project_event(
            "delete_cue_revision",
            "committed",
            "Cue revision deleted.",
            || serde_json::json!({ "cue_id": cue_id, "revision_id": revision_id }),
        );
        Ok(())
    }

    /// Persists a human-authored cue revision and marks it for review again.
    ///
    /// # Errors
    ///
    /// Returns an error when the cue is missing or the transaction fails.
    pub fn save_cue_revision(
        &self,
        cue_id: Uuid,
        document: &CueEditDocument,
    ) -> Result<CueRevision, ProjectError> {
        self.save_authored_revision(cue_id, document, RevisionAuthor::Human)
    }

    /// Persists an LLM-authored translation revision and invalidates the prior review.
    ///
    /// # Errors
    ///
    /// Returns an error when the cue is missing or the transaction fails.
    pub fn save_translation_revision(
        &self,
        cue_id: Uuid,
        document: &CueEditDocument,
    ) -> Result<CueRevision, ProjectError> {
        self.save_authored_revision(cue_id, document, RevisionAuthor::Translation)
    }

    fn save_authored_revision(
        &self,
        cue_id: Uuid,
        document: &CueEditDocument,
        author: RevisionAuthor,
    ) -> Result<CueRevision, ProjectError> {
        self.cue_edit_base(cue_id)?;
        let id = Uuid::new_v4();
        let timestamp = OffsetDateTime::now_utc().unix_timestamp();
        let created_at = OffsetDateTime::from_unix_timestamp(timestamp)
            .map_err(|_| ProjectError::InvalidRecord("revision timestamp".to_owned()))?;
        let serialized = serde_json::to_string(document)?;
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            self.connection.execute(
                "INSERT INTO cue_revisions (id, cue_id, author, document, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id.to_string(),
                    cue_id.to_string(),
                    author.as_str(),
                    serialized,
                    timestamp.to_string(),
                ],
            )?;
            self.connection.execute(
                "UPDATE cues SET review_status = 'unreviewed' WHERE id = ?1",
                params![cue_id.to_string()],
            )?;
            Ok::<_, ProjectError>(())
        })();
        if let Err(error) = result {
            let _ = self.connection.execute_batch("ROLLBACK");
            project_event(
                "save_revision",
                "rolled_back",
                "Revision transaction rolled back.",
                || serde_json::json!({ "cue_id": cue_id, "revision_id": id, "author": author.as_str(), "error": error.to_string() }),
            );
            return Err(error);
        }
        self.connection.execute_batch("COMMIT")?;
        project_event("save_revision", "committed", "Cue revision saved.", || {
            serde_json::json!({
                "cue_id": cue_id,
                "revision_id": id,
                "author": author.as_str(),
                "document": document
            })
        });
        Ok(CueRevision {
            id,
            cue_id,
            author,
            document: document.clone(),
            created_at,
        })
    }

    /// Returns the latest non-translation revision for every cue.
    ///
    /// These revisions form a stable source when a translated cue is translated again.
    ///
    /// # Errors
    ///
    /// Returns an error when stored revision data is invalid.
    pub fn translation_sources(&self) -> Result<Vec<CueRevision>, ProjectError> {
        let mut statement = self.connection.prepare(
            r"
            SELECT revision.id, revision.cue_id, revision.author, revision.document,
                   revision.created_at
            FROM cue_revisions AS revision
            JOIN cues AS cue ON cue.id = revision.cue_id
            WHERE revision.author != 'translation'
              AND revision.rowid = (
                SELECT MAX(latest.rowid)
                FROM cue_revisions AS latest
                WHERE latest.cue_id = revision.cue_id
                  AND latest.author != 'translation'
              )
            ORDER BY cue.track_id, cue.cue_index
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| parse_revision_row(&row?)).collect()
    }

    /// Returns the latest human review decision for every reviewed cue.
    ///
    /// # Errors
    ///
    /// Returns an error when a stored decision is invalid.
    pub fn review_decisions(&self) -> Result<Vec<CueReviewDecision>, ProjectError> {
        let mut statement = self.connection.prepare(
            r"
            SELECT decision.id, decision.cue_id, decision.revision_id,
                   decision.status, decision.note, decision.created_at
            FROM review_decisions AS decision
            WHERE decision.rowid = (
                SELECT MAX(latest.rowid)
                FROM review_decisions AS latest
                WHERE latest.cue_id = decision.cue_id
            )
            ORDER BY decision.rowid
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|row| parse_review_row(row?)).collect()
    }

    /// Records a review decision against the cue's latest effective revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the cue or revision is missing or persistence fails.
    pub fn save_review_decision(
        &self,
        cue_id: Uuid,
        status: ReviewStatus,
        note: &str,
    ) -> Result<CueReviewDecision, ProjectError> {
        self.cue_edit_base(cue_id)?;
        let revision_id = self
            .connection
            .query_row(
                "SELECT id FROM cue_revisions WHERE cue_id = ?1 ORDER BY rowid DESC LIMIT 1",
                params![cue_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| parse_uuid(&value, "review revision id"))
            .transpose()?;
        let id = Uuid::new_v4();
        let timestamp = OffsetDateTime::now_utc().unix_timestamp();
        let created_at = OffsetDateTime::from_unix_timestamp(timestamp)
            .map_err(|_| ProjectError::InvalidRecord("review timestamp".to_owned()))?;
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            self.connection.execute(
                "INSERT INTO review_decisions \
                 (id, cue_id, revision_id, status, note, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id.to_string(),
                    cue_id.to_string(),
                    revision_id.map(|value| value.to_string()),
                    status.as_str(),
                    note,
                    timestamp.to_string(),
                ],
            )?;
            self.connection.execute(
                "UPDATE cues SET review_status = ?1 WHERE id = ?2",
                params![status.as_str(), cue_id.to_string()],
            )?;
            Ok::<_, ProjectError>(())
        })();
        if let Err(error) = result {
            let _ = self.connection.execute_batch("ROLLBACK");
            project_event(
                "save_review_decision",
                "rolled_back",
                "Review transaction rolled back.",
                || serde_json::json!({ "cue_id": cue_id, "status": status.as_str(), "error": error.to_string() }),
            );
            return Err(error);
        }
        self.connection.execute_batch("COMMIT")?;
        project_event(
            "save_review_decision",
            "committed",
            "Cue review decision saved.",
            || {
                serde_json::json!({
                    "cue_id": cue_id,
                    "decision_id": id,
                    "revision_id": revision_id,
                    "status": status.as_str(),
                    "note": note
                })
            },
        );
        Ok(CueReviewDecision {
            id,
            cue_id,
            revision_id,
            status,
            note: note.to_owned(),
            created_at,
        })
    }

    /// Records a successfully written export artifact.
    ///
    /// # Errors
    ///
    /// Returns an error when settings cannot be serialized or the database
    /// insert fails.
    pub fn record_export(
        &self,
        format: &str,
        path: &str,
        settings: &serde_json::Value,
    ) -> Result<(), ProjectError> {
        self.connection.execute(
            "INSERT INTO exports (id, format, path, settings, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                format,
                path,
                serde_json::to_string(settings)?,
                OffsetDateTime::now_utc().unix_timestamp().to_string(),
            ],
        )?;
        project_event(
            "record_export",
            "committed",
            "Export record saved.",
            || serde_json::json!({ "format": format, "path": path, "settings": settings }),
        );
        Ok(())
    }

    /// Adds a durable background job to the project queue.
    ///
    /// # Errors
    ///
    /// Returns an error when request/progress serialization or insertion fails.
    pub fn enqueue_job(
        &self,
        kind: JobKind,
        request: &serde_json::Value,
        progress: &JobProgress,
    ) -> Result<ProjectJob, ProjectError> {
        let id = Uuid::new_v4();
        let timestamp = OffsetDateTime::now_utc().unix_timestamp();
        self.connection.execute(
            "INSERT INTO jobs (id, kind, status, progress, error, created_at, updated_at, request, result) \
             VALUES (?1, ?2, 'queued', ?3, NULL, ?4, ?4, ?5, NULL)",
            params![
                id.to_string(),
                kind.as_str(),
                serde_json::to_string(progress)?,
                timestamp.to_string(),
                serde_json::to_string(request)?,
            ],
        )?;
        project_event(
            "enqueue_job",
            "committed",
            "Background job queued.",
            || serde_json::json!({ "job_id": id, "kind": kind.as_str(), "request": request, "progress": progress }),
        );
        self.job(id)?
            .ok_or_else(|| ProjectError::InvalidRecord("queued job".to_owned()))
    }

    /// Lists durable jobs newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when stored job data is invalid.
    pub fn jobs(&self) -> Result<Vec<ProjectJob>, ProjectError> {
        let mut statement = self.connection.prepare(
            "SELECT id, kind, status, request, progress, error, result, created_at, updated_at \
             FROM jobs ORDER BY rowid DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        rows.map(|row| parse_job_row(row?)).collect()
    }

    /// Reads one durable job by ID.
    ///
    /// # Errors
    ///
    /// Returns an error when stored job data is invalid.
    pub fn job(&self, id: Uuid) -> Result<Option<ProjectJob>, ProjectError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, kind, status, request, progress, error, result, created_at, updated_at \
                 FROM jobs WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()?;
        row.map(parse_job_row).transpose()
    }

    /// Marks a queued or interrupted job as running.
    ///
    /// # Errors
    ///
    /// Returns an error when the job is missing or cannot be updated.
    pub fn start_job(&self, id: Uuid) -> Result<(), ProjectError> {
        self.update_job_status(id, JobStatus::Running, None, None)
    }

    /// Updates the checkpoint after a Cue-level commit.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization or persistence fails.
    pub fn update_job_progress(
        &self,
        id: Uuid,
        progress: &JobProgress,
    ) -> Result<(), ProjectError> {
        let updated = self.connection.execute(
            "UPDATE jobs SET progress = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(progress)?,
                OffsetDateTime::now_utc().unix_timestamp().to_string(),
                id.to_string(),
            ],
        )?;
        ensure_job_updated(updated)?;
        project_event(
            "update_job_progress",
            "committed",
            "Background job progress updated.",
            || serde_json::json!({ "job_id": id, "progress": progress }),
        );
        Ok(())
    }

    /// Finishes a job with a serializable result.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization or persistence fails.
    pub fn complete_job(&self, id: Uuid, result: &serde_json::Value) -> Result<(), ProjectError> {
        self.update_job_status(id, JobStatus::Completed, None, Some(result))
    }

    /// Records a terminal job error.
    ///
    /// # Errors
    ///
    /// Returns an error when the job is missing or cannot be updated.
    pub fn fail_job(&self, id: Uuid, error: &str) -> Result<(), ProjectError> {
        self.update_job_status(id, JobStatus::Failed, Some(error), None)
    }

    /// Marks a stopped job as resumable.
    ///
    /// # Errors
    ///
    /// Returns an error when the job is missing or cannot be updated.
    pub fn interrupt_job(&self, id: Uuid) -> Result<(), ProjectError> {
        self.update_job_status(id, JobStatus::Interrupted, None, None)
    }

    /// Marks an interrupted job as superseded or explicitly canceled.
    ///
    /// # Errors
    ///
    /// Returns an error when the job is missing or cannot be updated.
    pub fn cancel_job(
        &self,
        id: Uuid,
        result: Option<&serde_json::Value>,
    ) -> Result<(), ProjectError> {
        self.update_job_status(id, JobStatus::Canceled, None, result)
    }

    /// Converts stale in-process states into durable interrupted states after application restart.
    ///
    /// # Errors
    ///
    /// Returns an error when the update fails.
    pub fn recover_interrupted_jobs(&self) -> Result<u64, ProjectError> {
        let updated = self.connection.execute(
            "UPDATE jobs SET status = 'interrupted', updated_at = ?1 \
             WHERE status IN ('running', 'paused')",
            params![OffsetDateTime::now_utc().unix_timestamp().to_string()],
        )?;
        u64::try_from(updated)
            .map_err(|_| ProjectError::InvalidRecord("recovered job count".to_owned()))
    }

    fn update_job_status(
        &self,
        id: Uuid,
        status: JobStatus,
        error: Option<&str>,
        result: Option<&serde_json::Value>,
    ) -> Result<(), ProjectError> {
        let result = result.map(serde_json::to_string).transpose()?;
        let updated = self.connection.execute(
            "UPDATE jobs SET status = ?1, error = ?2, result = ?3, updated_at = ?4 WHERE id = ?5",
            params![
                status.as_str(),
                error,
                result,
                OffsetDateTime::now_utc().unix_timestamp().to_string(),
                id.to_string(),
            ],
        )?;
        ensure_job_updated(updated)?;
        project_event(
            "update_job_status",
            "committed",
            "Background job status updated.",
            || {
                serde_json::json!({
                    "job_id": id,
                    "status": status.as_str(),
                    "error": error,
                    "result": result
                })
            },
        );
        Ok(())
    }

    fn cue_edit_base(
        &self,
        cue_id: Uuid,
    ) -> Result<(u64, u64, rosettacue_domain::SubtitlePosition), ProjectError> {
        let row = self
            .connection
            .query_row(
                "SELECT start_ms, end_ms, geometry FROM cues WHERE id = ?1",
                params![cue_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or(ProjectError::CueNotFound)?;
        let start_ms = u64::try_from(row.0)
            .map_err(|_| ProjectError::InvalidRecord("cue start time".to_owned()))?;
        let end_ms = u64::try_from(row.1)
            .map_err(|_| ProjectError::InvalidRecord("cue end time".to_owned()))?;
        let geometry = serde_json::from_str::<CueGeometry>(&row.2)?;
        Ok((start_ms, end_ms, geometry.position()))
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn copy_directory_contents(source: &Path, destination: &Path) -> Result<(), ProjectError> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(ProjectError::UnsupportedSymlink(source_path));
        }
        if file_type.is_dir() {
            fs::create_dir(&destination_path)?;
            copy_directory_contents(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

type SourceRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
);

fn parse_source_row(row: SourceRow) -> Result<ProjectSource, ProjectError> {
    let id = parse_uuid(&row.0, "source id")?;
    let kind = match row.1.as_str() {
        "bluray_directory" => SourceKind::BlurayDirectory,
        value => return Err(ProjectError::InvalidRecord(format!("source kind {value}"))),
    };
    let metadata: SourceMetadata = serde_json::from_str(&row.5)?;
    let timestamp = row
        .6
        .parse::<i64>()
        .map_err(|_| ProjectError::InvalidRecord("source timestamp".to_owned()))?;
    let created_at = OffsetDateTime::from_unix_timestamp(timestamp)
        .map_err(|_| ProjectError::InvalidRecord("source timestamp".to_owned()))?;
    Ok(ProjectSource {
        id,
        kind,
        display_name: row.2,
        path: row.3,
        fingerprint: row.4,
        metadata,
        created_at,
    })
}

type TrackRow = (String, String, u32, Option<String>, String, String);

fn parse_track_row(row: TrackRow) -> Result<SubtitleTrack, ProjectError> {
    Ok(SubtitleTrack {
        id: parse_uuid(&row.0, "track id")?,
        source_id: parse_uuid(&row.1, "track source id")?,
        stream_index: row.2,
        language: row.3,
        codec: row.4,
        metadata: serde_json::from_str::<TrackMetadata>(&row.5)?,
    })
}

type CueRow = (
    String,
    String,
    u32,
    i64,
    i64,
    String,
    String,
    String,
    String,
    String,
);

fn parse_cue_row(row: CueRow) -> Result<SubtitleCue, ProjectError> {
    let ocr_status = match row.8.as_str() {
        "pending" => OcrStatus::Pending,
        "running" => OcrStatus::Running,
        "succeeded" => OcrStatus::Succeeded,
        "failed" => OcrStatus::Failed,
        value => return Err(ProjectError::InvalidRecord(format!("OCR status {value}"))),
    };
    let review_status = match row.9.as_str() {
        "unreviewed" => ReviewStatus::Unreviewed,
        "needs_review" => ReviewStatus::NeedsReview,
        "approved" => ReviewStatus::Approved,
        value => {
            return Err(ProjectError::InvalidRecord(format!(
                "review status {value}"
            )));
        }
    };
    let geometry = serde_json::from_str::<CueGeometry>(&row.7)?;
    Ok(SubtitleCue {
        id: parse_uuid(&row.0, "cue id")?,
        track_id: parse_uuid(&row.1, "cue track id")?,
        cue_index: row.2,
        start_ms: u64::try_from(row.3)
            .map_err(|_| ProjectError::InvalidRecord("cue start time".to_owned()))?,
        end_ms: u64::try_from(row.4)
            .map_err(|_| ProjectError::InvalidRecord("cue end time".to_owned()))?,
        image_path: row.5,
        image_sha256: row.6,
        position: geometry.position(),
        geometry,
        ocr_status,
        review_status,
    })
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, ProjectError> {
    Uuid::parse_str(value).map_err(|_| ProjectError::InvalidRecord(field.to_owned()))
}

fn parse_recognition_row(row: (String, String, String)) -> Result<CueRecognition, ProjectError> {
    let (cue_id, document, created_at) = row;
    let document = serde_json::from_str::<CueEditDocument>(&document)?;
    let timestamp = created_at
        .parse::<i64>()
        .map_err(|_| ProjectError::InvalidRecord("OCR revision timestamp".to_owned()))?;
    Ok(CueRecognition {
        cue_id: parse_uuid(&cue_id, "OCR revision cue id")?,
        document: document.subtitle,
        created_at: OffsetDateTime::from_unix_timestamp(timestamp)
            .map_err(|_| ProjectError::InvalidRecord("OCR revision timestamp".to_owned()))?,
    })
}

type RevisionRow = (String, String, String, String, String);

fn parse_revision_row(row: &RevisionRow) -> Result<CueRevision, ProjectError> {
    let author = match row.2.as_str() {
        "ocr" => RevisionAuthor::Ocr,
        "human" => RevisionAuthor::Human,
        "translation" => RevisionAuthor::Translation,
        value => {
            return Err(ProjectError::InvalidRecord(format!(
                "revision author {value}"
            )));
        }
    };
    let document = serde_json::from_str::<CueEditDocument>(&row.3)?;
    Ok(CueRevision {
        id: parse_uuid(&row.0, "revision id")?,
        cue_id: parse_uuid(&row.1, "revision cue id")?,
        author,
        document,
        created_at: parse_timestamp(&row.4, "revision timestamp")?,
    })
}

type JobRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

fn parse_job_row(row: JobRow) -> Result<ProjectJob, ProjectError> {
    let kind = match row.1.as_str() {
        "ocr" => JobKind::Ocr,
        "translation" => JobKind::Translation,
        "pgs_extraction" => JobKind::PgsExtraction,
        value => return Err(ProjectError::InvalidRecord(format!("job kind {value}"))),
    };
    let status = match row.2.as_str() {
        "queued" => JobStatus::Queued,
        "running" => JobStatus::Running,
        "paused" => JobStatus::Paused,
        "interrupted" => JobStatus::Interrupted,
        "completed" => JobStatus::Completed,
        "failed" => JobStatus::Failed,
        "canceled" => JobStatus::Canceled,
        value => return Err(ProjectError::InvalidRecord(format!("job status {value}"))),
    };
    let parse_time = |value: &str| {
        value
            .parse::<i64>()
            .map_err(|_| ProjectError::InvalidRecord("job timestamp".to_owned()))
            .and_then(|timestamp| {
                OffsetDateTime::from_unix_timestamp(timestamp)
                    .map_err(|_| ProjectError::InvalidRecord("job timestamp".to_owned()))
            })
    };
    Ok(ProjectJob {
        id: parse_uuid(&row.0, "job id")?,
        kind,
        status,
        request: serde_json::from_str(&row.3)?,
        progress: serde_json::from_str(&row.4)?,
        error: row.5,
        result: row
            .6
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        created_at: parse_time(&row.7)?,
        updated_at: parse_time(&row.8)?,
    })
}

fn ensure_job_updated(updated: usize) -> Result<(), ProjectError> {
    if updated == 1 {
        Ok(())
    } else {
        Err(ProjectError::InvalidRecord("job was not found".to_owned()))
    }
}

fn project_event(
    operation: &str,
    phase: &str,
    message: &str,
    details: impl FnOnce() -> serde_json::Value,
) {
    if !rosettacue_diagnostics::enabled() {
        return;
    }
    rosettacue_diagnostics::emit(DiagnosticEvent {
        level: if phase == "rolled_back" {
            DiagnosticLevel::Error
        } else {
            DiagnosticLevel::Debug
        },
        source: "project",
        category: "storage",
        operation,
        phase,
        message,
        duration_ms: None,
        details: details(),
    });
}

type ReviewRow = (String, String, Option<String>, String, String, String);

fn parse_review_row(row: ReviewRow) -> Result<CueReviewDecision, ProjectError> {
    let status = match row.3.as_str() {
        "unreviewed" => ReviewStatus::Unreviewed,
        "needs_review" => ReviewStatus::NeedsReview,
        "approved" => ReviewStatus::Approved,
        value => {
            return Err(ProjectError::InvalidRecord(format!(
                "review status {value}"
            )));
        }
    };
    Ok(CueReviewDecision {
        id: parse_uuid(&row.0, "review id")?,
        cue_id: parse_uuid(&row.1, "review cue id")?,
        revision_id: row
            .2
            .as_deref()
            .map(|value| parse_uuid(value, "review revision id"))
            .transpose()?,
        status,
        note: row.4,
        created_at: parse_timestamp(&row.5, "review timestamp")?,
    })
}

fn parse_timestamp(value: &str, field: &str) -> Result<OffsetDateTime, ProjectError> {
    let timestamp = value
        .parse::<i64>()
        .map_err(|_| ProjectError::InvalidRecord(field.to_owned()))?;
    OffsetDateTime::from_unix_timestamp(timestamp)
        .map_err(|_| ProjectError::InvalidRecord(field.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosettacue_domain::{OcrLine, OcrSpan, PROJECT_SCHEMA_VERSION, TextStyle};

    #[test]
    fn clones_project_with_new_identity_and_origin() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source_root = temporary.path().join("Original.rosettacue");
        let source = ProjectStore::create(&source_root, "Original").expect("create source");
        let source_metadata = source.metadata().expect("source metadata");
        fs::write(source_root.join("assets/cues/fixture.txt"), "fixture")
            .expect("write fixture asset");
        drop(source);

        let destination = temporary.path().join("Translation.rosettacue");
        let cloned = ProjectStore::clone_as(&source_root, &destination, "Translation")
            .expect("clone project");
        let metadata = cloned.metadata().expect("cloned metadata");
        assert_ne!(metadata.id, source_metadata.id);
        assert_eq!(metadata.name, "Translation");
        assert_eq!(
            metadata.origin.as_ref().map(|origin| origin.project_id),
            Some(source_metadata.id)
        );
        assert!(destination.join("assets/cues/fixture.txt").is_file());
    }

    #[test]
    fn stores_settings_in_only_the_selected_project() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let first = ProjectStore::create(temporary.path().join("First.rosettacue"), "First")
            .expect("create first project");
        let second = ProjectStore::create(temporary.path().join("Second.rosettacue"), "Second")
            .expect("create second project");
        let settings = ProjectSettings {
            ocr_language: "jpn".to_owned(),
            target_language: "eng".to_owned(),
            proper_nouns: vec![rosettacue_domain::ProperNounMapping {
                source: "綾瀬千早".to_owned(),
                translation: "Chihaya Ayase".to_owned(),
            }],
        };

        first.update_settings(&settings).expect("update settings");

        assert_eq!(first.metadata().expect("first metadata").settings, settings);
        assert_eq!(
            second.metadata().expect("second metadata").settings,
            ProjectSettings::default()
        );
    }

    #[test]
    fn rejects_every_non_current_schema_version() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let root = temporary.path().join("Old.rosettacue");
        let store = ProjectStore::create(&root, "Old").expect("create project");
        store
            .connection
            .pragma_update(None, "user_version", 99_u32)
            .expect("replace schema version");
        drop(store);

        assert!(matches!(
            ProjectStore::open(&root),
            Err(ProjectError::UnsupportedSchema {
                found: 99,
                expected: PROJECT_SCHEMA_VERSION
            })
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn creates_and_reopens_project() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let root = temporary.path().join("Movie.rosettacue");

        let created = ProjectStore::create(&root, "Movie").expect("create project");
        let created_metadata = created.metadata().expect("created metadata");
        assert_eq!(
            created.statistics().expect("created statistics"),
            ProjectStatistics::default()
        );
        let source = ProjectSource::from_bluray(rosettacue_domain::BlurayDiscInfo {
            root_path: "/Movies/Disc 1".to_owned(),
            display_name: "Disc 1".to_owned(),
            main_title_index: 1,
            titles: vec![rosettacue_domain::BlurayTitleInfo {
                index: 1,
                playlist: "00001".to_owned(),
                duration_seconds: 7_200,
                chapters: 12,
                angles: 1,
                clips: 1,
                video_tracks: 1,
                audio_tracks: 2,
                pgs_tracks: 1,
                pgs_languages: vec!["jpn".to_owned()],
            }],
        });
        created.add_source(&source).expect("add source");
        assert_eq!(created.statistics().expect("statistics").source_count, 1);
        let track = SubtitleTrack {
            id: Uuid::new_v4(),
            source_id: source.id,
            stream_index: 0,
            language: Some("jpn".to_owned()),
            codec: "hdmv_pgs_subtitle".to_owned(),
            metadata: TrackMetadata::Pgs(rosettacue_domain::PgsTrackMetadata {
                title_index: 1,
                playlist: "00001".to_owned(),
                sup_path: "assets/tracks/track/source.sup".to_owned(),
            }),
        };
        created.add_track(&track).expect("add track");
        let geometry = CueGeometry {
            canvas_width: 1_920,
            canvas_height: 1_080,
            x: 500,
            y: 800,
            width: 900,
            height: 120,
            image_width: 932,
            image_height: 152,
            forced: false,
            inferred_end: false,
        };
        let mut cue = SubtitleCue {
            id: Uuid::new_v4(),
            track_id: track.id,
            cue_index: 1,
            start_ms: 1_000,
            end_ms: 2_500,
            image_path: "assets/cues/track/000001.png".to_owned(),
            image_sha256: "abc123".to_owned(),
            position: geometry.position(),
            geometry,
            ocr_status: OcrStatus::Pending,
            review_status: ReviewStatus::Unreviewed,
        };
        created.add_cue(&cue).expect("add cue");
        assert_eq!(
            created.tracks().expect("tracks"),
            std::slice::from_ref(&track)
        );
        assert_eq!(created.cues().expect("cues"), std::slice::from_ref(&cue));
        let run_id = created
            .start_ocr_run(
                "lmstudio",
                "test-model",
                "test-prompt",
                "jpn",
                &serde_json::json!({}),
            )
            .expect("start OCR run");
        created
            .mark_cue_ocr_running(cue.id)
            .expect("mark OCR running");
        let document = OcrDocument {
            prompt_version: "test-prompt".to_owned(),
            provider: "lmstudio".to_owned(),
            model: "test-model".to_owned(),
            language: "jpn".to_owned(),
            unreadable: false,
            lines: vec![OcrLine {
                text: "字幕".to_owned(),
                spans: vec![OcrSpan::Text {
                    text: "字幕".to_owned(),
                    styles: Vec::new(),
                    color: None,
                }],
            }],
            normalizations: Vec::new(),
        };
        let recognition = created
            .save_ocr_success(cue.id, run_id, "{}", &document, 42)
            .expect("save OCR result");
        assert_eq!(recognition.document, document);
        assert_eq!(created.recognitions().expect("recognitions"), [recognition]);
        let edited = CueEditDocument {
            start_ms: 1_050,
            end_ms: 2_600,
            position: rosettacue_domain::SubtitlePosition::BottomCenter,
            subtitle: OcrDocument {
                lines: vec![OcrLine {
                    text: "字幕です".to_owned(),
                    spans: vec![OcrSpan::Text {
                        text: "字幕です".to_owned(),
                        styles: vec![TextStyle::Italic],
                        color: None,
                    }],
                }],
                ..document.clone()
            },
        };
        let revision = created
            .save_cue_revision(cue.id, &edited)
            .expect("save human revision");
        assert_eq!(revision.author, RevisionAuthor::Human);
        assert_eq!(
            created.revisions().expect("revisions"),
            std::slice::from_ref(&revision)
        );
        let decision = created
            .save_review_decision(cue.id, ReviewStatus::Approved, "verified")
            .expect("approve revision");
        assert_eq!(decision.revision_id, Some(revision.id));
        assert_eq!(
            created.review_decisions().expect("review decisions"),
            [decision]
        );
        assert_eq!(
            created
                .statistics()
                .expect("review statistics")
                .reviewed_count,
            1
        );
        let unreviewed = created
            .save_review_decision(cue.id, ReviewStatus::Unreviewed, "review reopened")
            .expect("reopen review");
        assert_eq!(unreviewed.status, ReviewStatus::Unreviewed);
        assert_eq!(
            created
                .statistics()
                .expect("reopened review statistics")
                .reviewed_count,
            0
        );
        assert_eq!(
            created
                .statistics()
                .expect("OCR statistics")
                .ocr_completed_count,
            1
        );
        let mut translated = edited.clone();
        translated.subtitle.language = "eng".to_owned();
        translated.subtitle.lines[0].text = "These are subtitles.".to_owned();
        translated.subtitle.lines[0].spans = vec![OcrSpan::Text {
            text: "These are subtitles.".to_owned(),
            styles: Vec::new(),
            color: None,
        }];
        let translation = created
            .save_translation_revision(cue.id, &translated)
            .expect("save translation revision");
        assert_eq!(translation.author, RevisionAuthor::Translation);
        let history = created
            .cue_revision_history(cue.id)
            .expect("cue revision history");
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].author, RevisionAuthor::Translation);
        assert_eq!(history[1].author, RevisionAuthor::Human);
        assert_eq!(history[2].author, RevisionAuthor::Ocr);
        assert_eq!(
            created.revision_counts().expect("revision counts")[&cue.id],
            3
        );
        assert_eq!(
            created.revisions().expect("effective translation"),
            std::slice::from_ref(&translation)
        );
        assert_eq!(
            created
                .translation_sources()
                .expect("stable translation source"),
            std::slice::from_ref(&revision)
        );
        cue.ocr_status = OcrStatus::Succeeded;
        cue.review_status = ReviewStatus::Unreviewed;
        let job = created
            .enqueue_job(
                JobKind::Ocr,
                &serde_json::json!({ "cue_ids": [cue.id] }),
                &JobProgress {
                    phase: "queued".to_owned(),
                    current: 0,
                    total: Some(1),
                    cue_id: None,
                    cue_index: None,
                    completed_cue_ids: Vec::new(),
                },
            )
            .expect("enqueue job");
        created.start_job(job.id).expect("start job");
        created
            .update_job_progress(
                job.id,
                &JobProgress {
                    phase: "running".to_owned(),
                    current: 1,
                    total: Some(1),
                    cue_id: Some(cue.id),
                    cue_index: Some(cue.cue_index),
                    completed_cue_ids: Vec::new(),
                },
            )
            .expect("checkpoint job");
        let serialized = serde_json::to_string(&created_metadata).expect("serialize metadata");
        assert!(serialized.contains('T'));
        drop(created);

        let reopened = ProjectStore::open(&root).expect("open project");
        assert_eq!(reopened.recover_interrupted_jobs().expect("recover job"), 1);
        let jobs = reopened.jobs().expect("jobs");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Interrupted);
        assert_eq!(reopened.metadata().expect("metadata"), created_metadata);
        assert_eq!(reopened.statistics().expect("statistics").source_count, 1);
        assert_eq!(reopened.statistics().expect("statistics").track_count, 1);
        assert_eq!(reopened.statistics().expect("statistics").cue_count, 1);
        assert_eq!(
            reopened
                .statistics()
                .expect("statistics")
                .ocr_completed_count,
            1
        );
        assert_eq!(reopened.tracks().expect("tracks"), [track]);
        assert_eq!(reopened.cues().expect("cues"), [cue.clone()]);
        assert_eq!(reopened.recognitions().expect("recognitions").len(), 1);
        assert_eq!(reopened.revisions().expect("revisions").len(), 1);
        assert_eq!(reopened.review_decisions().expect("reviews").len(), 1);
        reopened
            .delete_cue_revision(cue.id, translation.id)
            .expect("delete latest translation revision");
        assert_eq!(
            reopened
                .revisions()
                .expect("human revision becomes effective"),
            std::slice::from_ref(&revision)
        );
        reopened
            .delete_cue_revision(cue.id, revision.id)
            .expect("delete older human revision");
        let only_revision = reopened
            .cue_revision_history(cue.id)
            .expect("remaining OCR revision");
        assert_eq!(only_revision.len(), 1);
        assert!(matches!(
            reopened.delete_cue_revision(cue.id, only_revision[0].id),
            Err(ProjectError::LastRevision)
        ));
        assert!(root.join("assets/cues").is_dir());
        assert!(root.join("exports").is_dir());
    }
}
