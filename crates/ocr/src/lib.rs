mod error;
mod languages;
mod lmstudio;
mod prompt;
mod row_detection;

use std::path::PathBuf;

pub use error::OcrError;
pub use lmstudio::{
    LmStudioBackend, LmStudioConfig, LmStudioModel, OcrPipelineConfig, ProviderOcrBackend,
    diagnose_provider, list_lmstudio_models, list_provider_models,
};
pub use prompt::PROMPT_VERSION;
use rosettacue_domain::OcrDocument;
pub use rosettacue_llm::{LlmProvider, ProviderConfig, ProviderDiagnostic, ReasoningEffort};

#[derive(Debug, Clone)]
pub struct OcrRequest {
    pub cue_id: uuid::Uuid,
    pub cue_index: u32,
    pub image_path: PathBuf,
    pub image_sha256: String,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct OcrRecognition {
    pub document: OcrDocument,
    pub raw_response: String,
    pub elapsed_ms: u64,
}

pub trait OcrBackend {
    fn backend_id(&self) -> String;

    /// Recognizes one cue image and returns its structured subtitle document.
    ///
    /// # Errors
    ///
    /// Returns an error when the image cannot be read, the provider is unavailable,
    /// or the provider response does not satisfy the OCR contract.
    fn recognize(&self, request: &OcrRequest) -> Result<OcrRecognition, OcrError>;
}
