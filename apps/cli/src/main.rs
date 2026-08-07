use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rosettacue_core::{
    Application, ExportFormat, ExportOptions, ExportScope, LlmProvider, OcrPipelineConfig,
    ProviderConfig, ProviderSpec, ReasoningEffort,
};

#[derive(Debug, Parser)]
#[command(name = "rosettacue", version, about = "RosettaCue")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Version,
    Doctor,
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    Ocr {
        #[command(subcommand)]
        command: OcrCommand,
    },
    Translate {
        project: PathBuf,
        #[arg(long)]
        target_language: String,
        #[arg(long, value_enum, default_value_t = CliProvider::LmStudio)]
        provider: CliProvider,
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "http://127.0.0.1:1234/v1")]
        base_url: String,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long, value_enum)]
        reasoning_effort: Option<CliReasoningEffort>,
        #[arg(long)]
        cue_id: Vec<uuid::Uuid>,
        #[arg(long)]
        overwrite: bool,
    },
    Export {
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        track_id: Option<uuid::Uuid>,
        #[arg(long, value_enum, default_values_t = [CliExportFormat::Json, CliExportFormat::Srt])]
        format: Vec<CliExportFormat>,
        #[arg(long)]
        approved_only: bool,
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
}

impl From<CliReasoningEffort> for ReasoningEffort {
    fn from(value: CliReasoningEffort) -> Self {
        match value {
            CliReasoningEffort::None => Self::None,
            CliReasoningEffort::Minimal => Self::Minimal,
            CliReasoningEffort::Low => Self::Low,
            CliReasoningEffort::Medium => Self::Medium,
            CliReasoningEffort::High => Self::High,
        }
    }
}

/// Applies an explicit reasoning effort to the `OpenAI` profiles in a pipeline.
///
/// Only the `OpenAI` variant carries the parameter, so other providers are
/// skipped rather than failed — a pipeline may legitimately mix `OpenAI` with a
/// local or Anthropic stage. The flag is only an error when it would reach
/// nothing at all.
fn apply_reasoning_effort(
    configs: &mut [&mut ProviderConfig],
    effort: Option<CliReasoningEffort>,
) -> anyhow::Result<()> {
    let Some(effort) = effort else {
        return Ok(());
    };
    let mut applied = false;
    for config in configs {
        if let ProviderSpec::OpenAi { reasoning_effort } = &mut config.provider {
            *reasoning_effort = effort.into();
            applied = true;
        }
    }
    anyhow::ensure!(
        applied,
        "--reasoning-effort applies to openai profiles only, and none are configured"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliExportFormat {
    Json,
    Srt,
}

impl From<CliExportFormat> for ExportFormat {
    fn from(value: CliExportFormat) -> Self {
        match value {
            CliExportFormat::Json => Self::Json,
            CliExportFormat::Srt => Self::Srt,
        }
    }
}

#[derive(Debug, Subcommand)]
enum OcrCommand {
    Models {
        #[command(flatten)]
        provider: ProviderArgs,
    },
    Diagnose {
        #[command(flatten)]
        provider: ProviderArgs,
    },
    Run(Box<OcrRunArgs>),
}

#[derive(Debug, clap::Args)]
struct OcrRunArgs {
    project: PathBuf,
    #[arg(long, value_enum, default_value_t = CliProvider::LmStudio)]
    provider: CliProvider,
    #[arg(long)]
    model: String,
    #[arg(long, default_value = "http://127.0.0.1:1234/v1")]
    base_url: String,
    #[arg(long)]
    api_key_env: Option<String>,
    #[arg(long)]
    separate_ruby: bool,
    #[arg(long, value_enum, requires = "separate_ruby")]
    ruby_provider: Option<CliProvider>,
    #[arg(long, requires = "separate_ruby")]
    ruby_model: Option<String>,
    #[arg(long, requires = "separate_ruby")]
    ruby_base_url: Option<String>,
    #[arg(long, requires = "separate_ruby")]
    ruby_api_key_env: Option<String>,
    #[arg(long, value_enum)]
    validation_provider: Option<CliProvider>,
    #[arg(long)]
    validation_model: Option<String>,
    #[arg(long)]
    validation_base_url: Option<String>,
    #[arg(long)]
    validation_api_key_env: Option<String>,
    /// Applies to every `OpenAI` profile in the pipeline. Reasoning tokens bill at
    /// the output rate, so recognition defaults to `none`.
    #[arg(long, value_enum)]
    reasoning_effort: Option<CliReasoningEffort>,
    #[arg(long, default_value = "jpn")]
    language: String,
    #[arg(long)]
    cue_id: Vec<uuid::Uuid>,
    #[arg(long)]
    overwrite: bool,
}

#[derive(Debug, clap::Args)]
struct ProviderArgs {
    #[arg(long, value_enum, default_value_t = CliProvider::LmStudio)]
    provider: CliProvider,
    #[arg(long, default_value = "http://127.0.0.1:1234/v1")]
    base_url: String,
    #[arg(long)]
    api_key_env: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum CliProvider {
    LmStudio,
    Ollama,
    OpenAi,
    Anthropic,
}

fn task_provider_config(
    provider: CliProvider,
    inherited_provider: CliProvider,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    inherited: &ProviderConfig,
) -> ProviderConfig {
    let mut config = ProviderConfig::for_provider(provider.into());
    if provider == inherited_provider {
        config.base_url.clone_from(&inherited.base_url);
        config.model.clone_from(&inherited.model);
        config.api_key.clone_from(&inherited.api_key);
    }
    if let Some(base_url) = base_url {
        config.base_url = base_url;
    }
    if let Some(model) = model {
        config.model = model;
    }
    if api_key.is_some() {
        config.api_key = api_key;
    }
    config
}

impl From<CliProvider> for LlmProvider {
    fn from(value: CliProvider) -> Self {
        match value {
            CliProvider::LmStudio => Self::LmStudio,
            CliProvider::Ollama => Self::Ollama,
            CliProvider::OpenAi => Self::OpenAi,
            CliProvider::Anthropic => Self::Anthropic,
        }
    }
}

#[derive(Debug, Subcommand)]
enum SourceCommand {
    Inspect {
        path: PathBuf,
    },
    Attach {
        project: PathBuf,
        path: PathBuf,
    },
    Extract {
        project: PathBuf,
        source_id: uuid::Uuid,
        #[arg(long)]
        title: u32,
        #[arg(long, default_value_t = 0)]
        track: u32,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    Create {
        path: PathBuf,
        #[arg(long)]
        name: String,
    },
    Info {
        path: PathBuf,
    },
    SaveAs {
        project: PathBuf,
        parent: PathBuf,
        #[arg(long)]
        name: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    run_command(Application, Cli::parse().command)
}

#[allow(clippy::too_many_lines)]
fn run_command(app: Application, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Version => {
            println!("{}", serde_json::to_string_pretty(&app.backend_info())?);
        }
        Command::Doctor => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app.media_tool_diagnostics())?
            );
        }
        Command::Project {
            command: ProjectCommand::Create { path, name },
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app.create_project(path, &name)?)?
            );
        }
        Command::Project {
            command: ProjectCommand::Info { path },
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app.project_info(path)?)?
            );
        }
        Command::Project {
            command:
                ProjectCommand::SaveAs {
                    project,
                    parent,
                    name,
                },
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app.save_project_as(project, parent, &name)?)?
            );
        }
        Command::Source {
            command: SourceCommand::Inspect { path },
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app.inspect_bluray_source(path)?)?
            );
        }
        Command::Source {
            command: SourceCommand::Attach { project, path },
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app.attach_bluray_source(project, path)?)?
            );
        }
        Command::Source {
            command:
                SourceCommand::Extract {
                    project,
                    source_id,
                    title,
                    track,
                },
        } => {
            let result = app.extract_pgs_track(project, source_id, title, track, |progress| {
                if progress.phase == "decoding" {
                    eprint!("\rDecoded {} cues", progress.current);
                }
            })?;
            eprintln!();
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Ocr { command } => run_ocr_command(app, command)?,
        Command::Translate {
            project,
            target_language,
            provider,
            model,
            base_url,
            api_key_env,
            reasoning_effort,
            cue_id,
            overwrite,
        } => {
            let mut config = ProviderConfig {
                base_url,
                model,
                api_key: read_api_key(api_key_env.as_deref())?,
                ..ProviderConfig::for_provider(provider.into())
            };
            apply_reasoning_effort(&mut [&mut config], reasoning_effort)?;
            let result = app.translate_cues(
                project,
                (!cue_id.is_empty()).then_some(cue_id),
                &target_language,
                overwrite,
                &config,
                None,
                || true,
                |progress| {
                    eprint!(
                        "\rTranslation {}/{} ({})",
                        progress.current, progress.total, progress.phase
                    );
                },
            )?;
            eprintln!();
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Export {
            project,
            output,
            track_id,
            format,
            approved_only,
            name,
        } => run_export_command(app, project, output, track_id, format, approved_only, name)?,
    }
    Ok(())
}

fn run_ocr_command(app: Application, command: OcrCommand) -> anyhow::Result<()> {
    match command {
        OcrCommand::Models { provider } => {
            let api_key = read_api_key(provider.api_key_env.as_deref())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&app.provider_models(
                    provider.provider.into(),
                    &provider.base_url,
                    api_key.as_deref(),
                )?)?
            );
        }
        OcrCommand::Diagnose { provider } => {
            let api_key = read_api_key(provider.api_key_env.as_deref())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&app.diagnose_provider(
                    provider.provider.into(),
                    &provider.base_url,
                    api_key.as_deref(),
                ))?
            );
        }
        OcrCommand::Run(args) => run_ocr_pipeline(app, *args)?,
    }
    Ok(())
}

fn run_ocr_pipeline(app: Application, args: OcrRunArgs) -> anyhow::Result<()> {
    let OcrRunArgs {
        project,
        provider,
        model,
        base_url,
        api_key_env,
        separate_ruby,
        ruby_provider,
        ruby_model,
        ruby_base_url,
        ruby_api_key_env,
        validation_provider,
        validation_model,
        validation_base_url,
        validation_api_key_env,
        reasoning_effort,
        language,
        cue_id,
        overwrite,
    } = args;
    let mut recognition = ProviderConfig {
        base_url,
        model,
        api_key: read_api_key(api_key_env.as_deref())?,
        ..ProviderConfig::for_provider(provider.into())
    };
    let mut ruby = if separate_ruby {
        let ruby_provider = ruby_provider.unwrap_or(provider);
        Some(task_provider_config(
            ruby_provider,
            provider,
            ruby_base_url,
            ruby_model,
            read_api_key(ruby_api_key_env.as_deref())?,
            &recognition,
        ))
    } else {
        None
    };
    let validation_provider = validation_provider.unwrap_or(provider);
    let mut validation = ProviderConfig {
        base_url: validation_base_url.unwrap_or_else(|| recognition.base_url.clone()),
        model: validation_model.unwrap_or_else(|| recognition.model.clone()),
        api_key: read_api_key(validation_api_key_env.as_deref())?
            .or_else(|| recognition.api_key.clone()),
        ..ProviderConfig::for_provider(validation_provider.into())
    };
    let mut targets = vec![&mut recognition];
    targets.extend(ruby.as_mut());
    targets.push(&mut validation);
    apply_reasoning_effort(&mut targets, reasoning_effort)?;
    drop(targets);
    let selected = (!cue_id.is_empty()).then_some(cue_id);
    let result = app.recognize_ocr(
        project,
        selected,
        &language,
        overwrite,
        &OcrPipelineConfig {
            recognition,
            ruby,
            validation,
        },
        || true,
        |progress| {
            eprint!(
                "\rOCR {}/{} ({})",
                progress.current, progress.total, progress.phase
            );
        },
    )?;
    eprintln!();
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn read_api_key(environment_name: Option<&str>) -> anyhow::Result<Option<String>> {
    environment_name
        .map(|name| {
            std::env::var(name)
                .map_err(|error| anyhow::anyhow!("could not read API key from {name}: {error}"))
        })
        .transpose()
}

fn run_export_command(
    app: Application,
    project: PathBuf,
    output: PathBuf,
    track_id: Option<uuid::Uuid>,
    format: Vec<CliExportFormat>,
    approved_only: bool,
    name: Option<String>,
) -> anyhow::Result<()> {
    let result = app.export_subtitles(
        project,
        &ExportOptions {
            track_id,
            formats: format.into_iter().map(Into::into).collect(),
            scope: if approved_only {
                ExportScope::ApprovedOnly
            } else {
                ExportScope::AllRecognized
            },
            output_directory: output,
            base_name: name,
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_provider_task_inherits_the_recognition_profile() {
        let recognition = ProviderConfig {
            base_url: "https://gateway.example/v1".to_owned(),
            model: "vision-main".to_owned(),
            api_key: Some("session-key".to_owned()),
            ..ProviderConfig::for_provider(LlmProvider::OpenAi)
        };

        let config = task_provider_config(
            CliProvider::OpenAi,
            CliProvider::OpenAi,
            None,
            None,
            None,
            &recognition,
        );

        assert_eq!(config.base_url, recognition.base_url);
        assert_eq!(config.model, recognition.model);
        assert_eq!(config.api_key, recognition.api_key);
    }

    #[test]
    fn different_provider_task_uses_its_own_defaults_and_credentials() {
        let recognition = ProviderConfig {
            base_url: "https://api.openai.com/v1".to_owned(),
            model: "vision-main".to_owned(),
            api_key: Some("openai-session-key".to_owned()),
            ..ProviderConfig::for_provider(LlmProvider::OpenAi)
        };

        let config = task_provider_config(
            CliProvider::Anthropic,
            CliProvider::OpenAi,
            None,
            Some("claude-vision".to_owned()),
            Some("anthropic-session-key".to_owned()),
            &recognition,
        );

        assert_eq!(config.base_url, LlmProvider::Anthropic.default_base_url());
        assert_eq!(config.model, "claude-vision");
        assert_eq!(config.api_key.as_deref(), Some("anthropic-session-key"));
        assert_ne!(config.api_key, recognition.api_key);
    }
}
