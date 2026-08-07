use reqwest::blocking::Client;

use crate::{
    CompletionRequest, CompletionResponse, LlmError, LlmModel, ProviderConfig, ProviderSpec,
};

mod anthropic;
mod lmstudio;
mod ollama;
mod openai;
mod openai_compatible;

pub(super) fn complete(
    client: &Client,
    config: &ProviderConfig,
    request: &CompletionRequest<'_>,
) -> Result<CompletionResponse, LlmError> {
    match config.provider {
        ProviderSpec::LmStudio => lmstudio::complete(client, config, request),
        ProviderSpec::Ollama => ollama::complete(client, config, request),
        ProviderSpec::OpenAi { reasoning_effort } => {
            openai::complete(client, config, request, reasoning_effort)
        }
        ProviderSpec::Anthropic => anthropic::complete(client, config, request),
    }
}

pub(super) fn list_models(
    client: &Client,
    config: &ProviderConfig,
) -> Result<Vec<LlmModel>, LlmError> {
    match config.provider {
        ProviderSpec::LmStudio => lmstudio::list_models(client, config),
        ProviderSpec::Ollama => ollama::list_models(client, config),
        ProviderSpec::OpenAi { .. } => openai::list_models(client, config),
        ProviderSpec::Anthropic => anthropic::list_models(client, config),
    }
}
