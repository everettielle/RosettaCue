use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("project directory is not empty: {0}")]
    DirectoryNotEmpty(PathBuf),
    #[error("project destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("project destination cannot be inside the source project: {0}")]
    DestinationInsideSource(PathBuf),
    #[error("project package contains an unsupported symbolic link: {0}")]
    UnsupportedSymlink(PathBuf),
    #[error("directory is not a RosettaCue project: {0}")]
    NotAProject(PathBuf),
    #[error("project metadata is missing")]
    MissingMetadata,
    #[error("project schema {found} is unsupported; this build requires schema {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("cue was not found in the project")]
    CueNotFound,
    #[error("project statistics contain an invalid count")]
    InvalidStatistics,
    #[error("project contains an invalid record: {0}")]
    InvalidRecord(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
