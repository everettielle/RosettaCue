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
pub use rosettacue_layout::{CueLayout, LayoutOptions};
pub use rosettacue_llm::{
    LlmProvider, ProviderConfig, ProviderDiagnostic, ProviderSpec, ReasoningEffort,
};

/// The layout options a language implies.
///
/// Reading order is language policy, so it is resolved from the same preset
/// table the prompts come from rather than being chosen at each call site.
///
/// # Errors
///
/// Returns an error when the language has no preset.
pub fn layout_options(language: &str) -> Result<LayoutOptions, OcrError> {
    Ok(LayoutOptions {
        block_order: languages::resolve(language)?.block_order,
        ..LayoutOptions::default()
    })
}

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
