use rosettacue_domain::PROJECT_SCHEMA_VERSION;
use rusqlite::Connection;

use crate::ProjectError;

pub fn initialize(connection: &Connection) -> Result<(), ProjectError> {
    connection.execute_batch(&format!(
        r"
            PRAGMA foreign_keys = ON;

            CREATE TABLE project_metadata (
                id       INTEGER PRIMARY KEY CHECK (id = 1),
                document TEXT NOT NULL CHECK (json_valid(document))
            );

            CREATE TABLE sources (
                id           TEXT PRIMARY KEY,
                kind         TEXT NOT NULL,
                display_name TEXT NOT NULL,
                path         TEXT NOT NULL,
                fingerprint  TEXT,
                created_at   TEXT NOT NULL,
                metadata     TEXT NOT NULL CHECK (json_valid(metadata))
            );
            CREATE UNIQUE INDEX sources_kind_path ON sources(kind, path);

            CREATE TABLE tracks (
                id           TEXT PRIMARY KEY,
                source_id    TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                stream_index INTEGER NOT NULL,
                language     TEXT,
                codec        TEXT NOT NULL,
                metadata     TEXT NOT NULL CHECK (json_valid(metadata))
            );

            CREATE TABLE cues (
                id            TEXT PRIMARY KEY,
                track_id      TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                cue_index     INTEGER NOT NULL,
                start_ms      INTEGER NOT NULL,
                end_ms        INTEGER NOT NULL,
                image_path    TEXT NOT NULL,
                image_sha256  TEXT NOT NULL,
                geometry      TEXT NOT NULL CHECK (json_valid(geometry)),
                ocr_status    TEXT NOT NULL DEFAULT 'pending',
                review_status TEXT NOT NULL DEFAULT 'unreviewed',
                UNIQUE(track_id, cue_index)
            );

            CREATE TABLE ocr_runs (
                id             TEXT PRIMARY KEY,
                provider       TEXT NOT NULL,
                model          TEXT NOT NULL,
                prompt_version TEXT NOT NULL,
                language       TEXT,
                settings       TEXT NOT NULL CHECK (json_valid(settings)),
                created_at     TEXT NOT NULL
            );

            CREATE TABLE ocr_attempts (
                id             TEXT PRIMARY KEY,
                cue_id         TEXT NOT NULL REFERENCES cues(id) ON DELETE CASCADE,
                run_id         TEXT NOT NULL REFERENCES ocr_runs(id) ON DELETE CASCADE,
                attempt_number INTEGER NOT NULL,
                status         TEXT NOT NULL,
                raw_response   TEXT,
                candidate      TEXT CHECK (candidate IS NULL OR json_valid(candidate)),
                issues         TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(issues)),
                elapsed_ms     INTEGER,
                created_at     TEXT NOT NULL,
                UNIQUE(cue_id, run_id, attempt_number)
            );

            CREATE TABLE cue_revisions (
                id         TEXT PRIMARY KEY,
                cue_id     TEXT NOT NULL REFERENCES cues(id) ON DELETE CASCADE,
                author     TEXT NOT NULL,
                document   TEXT NOT NULL CHECK (json_valid(document)),
                created_at TEXT NOT NULL
            );

            CREATE TABLE review_decisions (
                id          TEXT PRIMARY KEY,
                cue_id      TEXT NOT NULL REFERENCES cues(id) ON DELETE CASCADE,
                revision_id TEXT REFERENCES cue_revisions(id) ON DELETE SET NULL,
                status      TEXT NOT NULL,
                note        TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL
            );

            CREATE TABLE jobs (
                id         TEXT PRIMARY KEY,
                kind       TEXT NOT NULL,
                status     TEXT NOT NULL,
                request    TEXT NOT NULL CHECK (json_valid(request)),
                progress   TEXT NOT NULL CHECK (json_valid(progress)),
                error      TEXT,
                result     TEXT CHECK (result IS NULL OR json_valid(result)),
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE exports (
                id         TEXT PRIMARY KEY,
                format     TEXT NOT NULL,
                path       TEXT NOT NULL,
                settings   TEXT NOT NULL CHECK (json_valid(settings)),
                created_at TEXT NOT NULL
            );

            PRAGMA user_version = {PROJECT_SCHEMA_VERSION};
        "
    ))?;
    Ok(())
}

pub fn validate(connection: &Connection) -> Result<(), ProjectError> {
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    let found = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found != PROJECT_SCHEMA_VERSION {
        return Err(ProjectError::UnsupportedSchema {
            found,
            expected: PROJECT_SCHEMA_VERSION,
        });
    }
    Ok(())
}
