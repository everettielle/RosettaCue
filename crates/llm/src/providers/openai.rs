use reqwest::blocking::Client;

use super::openai_compatible;
use crate::{
    CompletionRequest, CompletionResponse, LlmError, LlmModel, ProviderConfig, ReasoningEffort,
};

pub(super) fn complete(
    client: &Client,
    config: &ProviderConfig,
    request: &CompletionRequest<'_>,
    reasoning_effort: ReasoningEffort,
) -> Result<CompletionResponse, LlmError> {
    openai_compatible::complete(
        client,
        config,
        request,
        openai_compatible::Dialect::OpenAi { reasoning_effort },
    )
}

pub(super) fn list_models(
    client: &Client,
    config: &ProviderConfig,
) -> Result<Vec<LlmModel>, LlmError> {
    openai_compatible::list_models(client, config)
}
