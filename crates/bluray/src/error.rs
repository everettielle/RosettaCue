use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum BlurayError {
    #[error("source directory does not exist: {0}")]
    SourceNotFound(PathBuf),
    #[error("not a Blu-ray backup: {0}; expected BDMV/index.bdmv")]
    NotBluray(PathBuf),
    #[error("required tool '{0}' was not found")]
    ToolNotFound(&'static str),
    #[error("{tool} failed with exit code {status}: {message}")]
    ToolFailed {
        tool: &'static str,
        status: i32,
        message: String,
    },
    #[error("could not read tool output: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    #[error("invalid bd_list_titles output: {0}")]
    InvalidOutput(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
