use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PgsError {
    #[error("SUP file does not exist: {0}")]
    SourceNotFound(PathBuf),
    #[error("invalid PGS stream: {0}")]
    InvalidStream(String),
    #[error("PNG encoding failed: {0}")]
    Png(#[from] png::EncodingError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
