use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rosettacue_core::{Application, ExportFormat, ExportOptions, ExportScope, LlmProvider};

mod model_config;

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
        /// Path to a model profile JSON document, or the document itself when
        /// the value starts with '{'.
        #[arg(long)]
        config: String,
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
    /// Analyzes every cue bitmap and reports the layout breakdown without
    /// contacting a provider.
    LayoutSurvey {
        project: PathBuf,
        #[arg(long, default_value = "jpn")]
        language: String,
        /// Write the full report here instead of printing it.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    Run(Box<OcrRunArgs>),
}

#[derive(Debug, clap::Args)]
struct OcrRunArgs {
    project: PathBuf,
    /// Path to a model config JSON document describing the recognition,
    /// optional ruby, and validation profiles — or the document itself when
    /// the value starts with '{'.
    #[arg(long)]
    config: String,
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
            config,
            cue_id,
            overwrite,
        } => {
            let config = model_config::load_profile(&config)?;
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
        OcrCommand::LayoutSurvey {
            project,
            language,
            json,
        } => {
            let survey = app.survey_cue_layout(project, &language)?;
            let report = serde_json::to_string_pretty(&survey)?;
            if let Some(path) = json {
                std::fs::write(&path, report)?;
                eprintln!(
                    "Analyzed {} cues: {} multi-block, {} vertical, {} mixed, {} degraded. Report written to {}.",
                    survey.cue_count,
                    survey
                        .cues_by_block_count
                        .iter()
                        .filter(|(blocks, _)| **blocks > 1)
                        .map(|(_, cues)| cues)
                        .sum::<u32>(),
                    survey.vertical_cues,
                    survey.mixed_direction_cues,
                    survey.degraded_cues,
                    path.display()
                );
            } else {
                println!("{report}");
            }
        }
        OcrCommand::Run(args) => run_ocr_pipeline(app, *args)?,
    }
    Ok(())
}

fn run_ocr_pipeline(app: Application, args: OcrRunArgs) -> anyhow::Result<()> {
    let OcrRunArgs {
        project,
        config,
        language,
        cue_id,
        overwrite,
    } = args;
    let pipeline = model_config::load_pipeline(&config)?;
    let selected = (!cue_id.is_empty()).then_some(cue_id);
    let result = app.recognize_ocr(
        project,
        selected,
        &language,
        overwrite,
        &pipeline,
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
