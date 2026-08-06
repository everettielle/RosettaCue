use reqwest::blocking::Client;

use crate::{
    CompletionRequest, CompletionResponse, LlmError, LlmModel, LlmProvider, ProviderConfig,
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
        LlmProvider::LmStudio => lmstudio::complete(client, config, request),
        LlmProvider::Ollama => ollama::complete(client, config, request),
        LlmProvider::OpenAi => openai::complete(client, config, request),
        LlmProvider::Anthropic => anthropic::complete(client, config, request),
    }
}

pub(super) fn list_models(
    client: &Client,
    config: &ProviderConfig,
) -> Result<Vec<LlmModel>, LlmError> {
    match config.provider {
        LlmProvider::LmStudio => lmstudio::list_models(client, config),
        LlmProvider::Ollama => ollama::list_models(client, config),
        LlmProvider::OpenAi => openai::list_models(client, config),
        LlmProvider::Anthropic => anthropic::list_models(client, config),
    }
}
