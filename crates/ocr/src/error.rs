#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("invalid OCR configuration: {0}")]
    InvalidConfig(String),
    #[error("could not reach OCR provider: {0}")]
    Unavailable(String),
    #[error("OCR provider returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("OCR provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("OCR response failed validation: {0}")]
    Validation(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Provider(#[from] rosettacue_llm::LlmError),
}
