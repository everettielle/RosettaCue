use reqwest::blocking::Client;

use super::openai_compatible;
use crate::{CompletionRequest, CompletionResponse, LlmError, LlmModel, ProviderConfig};

pub(super) fn complete(
    client: &Client,
    config: &ProviderConfig,
    request: &CompletionRequest<'_>,
) -> Result<CompletionResponse, LlmError> {
    openai_compatible::complete(
        client,
        config,
        request,
        openai_compatible::Dialect::LocalCompatible,
    )
}

pub(super) fn list_models(
    client: &Client,
    config: &ProviderConfig,
) -> Result<Vec<LlmModel>, LlmError> {
    openai_compatible::list_models(client, config)
}
