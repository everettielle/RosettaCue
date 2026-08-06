use std::io::{BufRead, Write};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, mpsc};

use rosettacue_core::{
    Application, ExportOptions, LlmProvider, LmStudioConfig, OcrPipelineConfig, ProviderConfig,
    configure_media_tools_directory,
};
use rosettacue_diagnostics::{DiagnosticEvent, DiagnosticLevel};
use rosettacue_domain::{CueEditDocument, ProjectSettings, ReviewStatus};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum OcrControlState {
    #[default]
    Idle,
    Running,
    Paused,
    Stopping,
}

#[derive(Clone, Default)]
struct OcrJobController {
    inner: Arc<(Mutex<OcrControlState>, Condvar)>,
}

impl OcrJobController {
    fn start(&self) -> Result<(), String> {
        let mut state = self.lock();
        if *state != OcrControlState::Idle {
            return Err("an OCR job is already active".to_owned());
        }
        *state = OcrControlState::Running;
        Ok(())
    }

    fn pause(&self) -> Result<(), String> {
        let mut state = self.lock();
        match *state {
            OcrControlState::Running => *state = OcrControlState::Paused,
            OcrControlState::Paused => {}
            OcrControlState::Idle => return Err("there is no active OCR job".to_owned()),
            OcrControlState::Stopping => return Err("the OCR job is stopping".to_owned()),
        }
        Ok(())
    }

    fn resume(&self) -> Result<(), String> {
        let mut state = self.lock();
        match *state {
            OcrControlState::Paused => {
                *state = OcrControlState::Running;
                self.inner.1.notify_all();
            }
            OcrControlState::Running => {}
            OcrControlState::Idle => return Err("there is no active OCR job".to_owned()),
            OcrControlState::Stopping => return Err("the OCR job is stopping".to_owned()),
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        let mut state = self.lock();
        match *state {
            OcrControlState::Running | OcrControlState::Paused => {
                *state = OcrControlState::Stopping;
                self.inner.1.notify_all();
            }
            OcrControlState::Stopping => {}
            OcrControlState::Idle => return Err("there is no active OCR job".to_owned()),
        }
        Ok(())
    }

    fn wait_until_runnable(&self, on_paused: impl FnOnce()) -> bool {
        let mut state = self.lock();
        if *state == OcrControlState::Paused {
            on_paused();
        }
        while *state == OcrControlState::Paused {
            state = self
                .inner
                .1
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *state == OcrControlState::Running
    }

    fn finish(&self) {
        *self.lock() = OcrControlState::Idle;
        self.inner.1.notify_all();
    }

    fn lock(&self) -> MutexGuard<'_, OcrControlState> {
        self.inner.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct RpcEvent<'a, T> {
    event: &'a str,
    payload: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectParams {
    parent: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveProjectAsParams {
    project_path: String,
    parent: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProjectSettingsParams {
    project_path: String,
    settings: ProjectSettings,
}

#[derive(Debug, Deserialize)]
struct PathParams {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportSubtitlesParams {
    project_path: String,
    options: ExportOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachSourceParams {
    project_path: String,
    source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtractPgsParams {
    project_path: String,
    source_id: Uuid,
    title_index: u32,
    stream_index: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CueImageParams {
    project_path: String,
    image_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveCueEditParams {
    project_path: String,
    cue_id: Uuid,
    document: CueEditDocument,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CueParams {
    project_path: String,
    cue_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreCueRevisionParams {
    project_path: String,
    cue_id: Uuid,
    revision_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewCueParams {
    project_path: String,
    cue_id: Uuid,
    status: ReviewStatus,
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LmStudioModelsParams {
    base_url: String,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderParams {
    provider: LlmProvider,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecognizeLmStudioParams {
    project_path: String,
    cue_ids: Option<Vec<Uuid>>,
    language: String,
    overwrite: bool,
    config: LmStudioConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecognizeOcrParams {
    project_path: String,
    cue_ids: Option<Vec<Uuid>>,
    language: String,
    overwrite: bool,
    config: OcrPipelineConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslateParams {
    project_path: String,
    cue_ids: Option<Vec<Uuid>>,
    target_language: String,
    overwrite: bool,
    config: ProviderConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectPathParams {
    project_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectJobParams {
    project_path: String,
    job_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeOcrParams {
    project_path: String,
    job_id: Uuid,
    config: OcrPipelineConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeTranslationParams {
    project_path: String,
    job_id: Uuid,
    config: ProviderConfig,
}

#[derive(Debug, Deserialize)]
struct ConfigureDiagnosticsParams {
    enabled: bool,
}

type MessageSender = mpsc::Sender<Value>;

fn main() {
    if let Ok(path) = std::env::var("ROSETTACUE_MEDIA_TOOLS_DIR") {
        configure_media_tools_directory(path);
    }

    let (sender, receiver) = mpsc::channel::<Value>();
    std::thread::spawn(move || write_messages(receiver));
    let diagnostic_sender = sender.clone();
    let _ = rosettacue_diagnostics::set_sink(std::sync::Arc::new(move |entry| {
        emit(&diagnostic_sender, "diagnostic-log", entry);
    }));
    rosettacue_diagnostics::configure(
        std::env::var("ROSETTACUE_DEBUG_LOGGING").is_ok_and(|value| value == "1"),
    );
    let controller = OcrJobController::default();
    let stdin = std::io::stdin();

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                send_value(
                    &sender,
                    json!({
                        "id": Value::Null,
                        "error": { "code": "invalid_request", "message": error.to_string() }
                    }),
                );
                continue;
            }
        };
        let request_sender = sender.clone();
        let request_controller = controller.clone();
        std::thread::spawn(move || {
            handle_request(request, &request_sender, &request_controller);
        });
    }
}

fn handle_request(request: RpcRequest, sender: &MessageSender, controller: &OcrJobController) {
    let id = request.id;
    let correlation_id = id.clone();
    let method = request.method;
    let started = std::time::Instant::now();
    let result = rosettacue_diagnostics::with_correlation(correlation_id, || {
        if rosettacue_diagnostics::enabled() {
            rosettacue_diagnostics::emit(DiagnosticEvent {
                level: DiagnosticLevel::Debug,
                source: "backend",
                category: "rpc",
                operation: &method,
                phase: "dispatch",
                message: "Dispatching backend request.",
                duration_ms: None,
                details: json!({}),
            });
        }
        let result = dispatch(&method, request.params, sender, controller);
        if rosettacue_diagnostics::enabled() {
            rosettacue_diagnostics::emit(DiagnosticEvent {
                level: if result.is_ok() {
                    DiagnosticLevel::Debug
                } else {
                    DiagnosticLevel::Error
                },
                source: "backend",
                category: "rpc",
                operation: &method,
                phase: if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                },
                message: if result.is_ok() {
                    "Backend request completed."
                } else {
                    "Backend request failed."
                },
                duration_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
                details: result
                    .as_ref()
                    .err()
                    .map_or_else(|| json!({}), |error| json!({ "error": error })),
            });
        }
        result
    });
    let response = match result {
        Ok(result) => RpcResponse {
            id,
            result: Some(result),
            error: None,
        },
        Err(message) => RpcResponse {
            id,
            result: None,
            error: Some(RpcError {
                code: "backend_error",
                message,
            }),
        },
    };
    send_serializable(sender, &response);
}

#[allow(clippy::too_many_lines)]
fn dispatch(
    method: &str,
    value: Value,
    sender: &MessageSender,
    controller: &OcrJobController,
) -> Result<Value, String> {
    let app = Application;
    match method {
        "backend_info" => serialize(app.backend_info()),
        "media_tool_diagnostics" => serialize(app.media_tool_diagnostics()),
        "create_project" => {
            let params: CreateProjectParams = parse(value)?;
            let name = params.name.trim();
            if name.is_empty() || name.contains(['/', '\\']) {
                return Err("a project name cannot be empty or contain a path separator".to_owned());
            }
            serialize_result(app.create_project(
                std::path::PathBuf::from(params.parent).join(format!("{name}.rosettacue")),
                name,
            ))
        }
        "save_project_as" => {
            let params: SaveProjectAsParams = parse(value)?;
            serialize_result(app.save_project_as(params.project_path, params.parent, &params.name))
        }
        "open_project" => {
            let params: PathParams = parse(value)?;
            serialize_result(app.project_info(params.path))
        }
        "project_document" => {
            let params: PathParams = parse(value)?;
            serialize_result(app.project_document(params.path))
        }
        "update_project_settings" => {
            let params: UpdateProjectSettingsParams = parse(value)?;
            serialize_result(app.update_project_settings(params.project_path, &params.settings))
        }
        "export_subtitles" => {
            let params: ExportSubtitlesParams = parse(value)?;
            serialize_result(app.export_subtitles(params.project_path, &params.options))
        }
        "inspect_bluray_source" => {
            let params: PathParams = parse(value)?;
            serialize_result(app.inspect_bluray_source(params.path))
        }
        "attach_bluray_source" => {
            let params: AttachSourceParams = parse(value)?;
            serialize_result(app.attach_bluray_source(params.project_path, params.source_path))
        }
        "extract_pgs_track" => {
            let params: ExtractPgsParams = parse(value)?;
            let event_sender = sender.clone();
            serialize_result(app.extract_pgs_track(
                params.project_path,
                params.source_id,
                params.title_index,
                params.stream_index,
                move |progress| emit(&event_sender, "pgs-extraction-progress", progress),
            ))
        }
        "cue_image" => {
            let params: CueImageParams = parse(value)?;
            serialize_result(app.cue_image(params.project_path, params.image_path))
        }
        "save_cue_edit" => {
            let params: SaveCueEditParams = parse(value)?;
            serialize_result(app.save_cue_edit(
                params.project_path,
                params.cue_id,
                &params.document,
            ))
        }
        "restore_cue_edit" => {
            let params: CueParams = parse(value)?;
            serialize_result(app.restore_cue_edit(params.project_path, params.cue_id))
        }
        "cue_revision_history" => {
            let params: CueParams = parse(value)?;
            serialize_result(app.cue_revision_history(params.project_path, params.cue_id))
        }
        "restore_cue_revision" => {
            let params: RestoreCueRevisionParams = parse(value)?;
            serialize_result(app.restore_cue_revision(
                params.project_path,
                params.cue_id,
                params.revision_id,
            ))
        }
        "delete_cue_revision" => {
            let params: RestoreCueRevisionParams = parse(value)?;
            serialize_result(app.delete_cue_revision(
                params.project_path,
                params.cue_id,
                params.revision_id,
            ))
        }
        "review_cue" => {
            let params: ReviewCueParams = parse(value)?;
            serialize_result(app.review_cue(
                params.project_path,
                params.cue_id,
                params.status,
                &params.note,
            ))
        }
        "lmstudio_models" => {
            let params: LmStudioModelsParams = parse(value)?;
            serialize_result(app.lmstudio_models(&params.base_url, params.api_key.as_deref()))
        }
        "provider_models" => {
            let params: ProviderParams = parse(value)?;
            serialize_result(app.provider_models(
                params.provider,
                &params.base_url,
                params.api_key.as_deref(),
            ))
        }
        "diagnose_provider" => {
            let params: ProviderParams = parse(value)?;
            serialize(app.diagnose_provider(
                params.provider,
                &params.base_url,
                params.api_key.as_deref(),
            ))
        }
        "recognize_lmstudio" => {
            let params: RecognizeLmStudioParams = parse(value)?;
            controller.start()?;
            let event_sender = sender.clone();
            let paused_sender = sender.clone();
            let result = app.recognize_lmstudio(
                params.project_path,
                params.cue_ids,
                &params.language,
                params.overwrite,
                &params.config,
                || {
                    controller.wait_until_runnable(|| {
                        emit(&paused_sender, "ocr-control-state", "paused");
                    })
                },
                move |progress| emit(&event_sender, "ocr-progress", progress),
            );
            controller.finish();
            serialize_result(result)
        }
        "recognize_ocr" => {
            let params: RecognizeOcrParams = parse(value)?;
            controller.start()?;
            let event_sender = sender.clone();
            let paused_sender = sender.clone();
            let result = app.recognize_ocr(
                params.project_path,
                params.cue_ids,
                &params.language,
                params.overwrite,
                &params.config,
                || {
                    controller.wait_until_runnable(|| {
                        emit(&paused_sender, "ocr-control-state", "paused");
                    })
                },
                move |progress| emit(&event_sender, "ocr-progress", progress),
            );
            controller.finish();
            serialize_result(result)
        }
        "translate_cues" => {
            let params: TranslateParams = parse(value)?;
            let event_sender = sender.clone();
            serialize_result(app.translate_cues(
                params.project_path,
                params.cue_ids,
                &params.target_language,
                params.overwrite,
                &params.config,
                None,
                || true,
                move |progress| emit(&event_sender, "translation-progress", progress),
            ))
        }
        "project_jobs" => {
            let params: ProjectPathParams = parse(value)?;
            serialize_result(app.project_jobs(params.project_path, true))
        }
        "cancel_project_job" => {
            let params: ProjectJobParams = parse(value)?;
            serialize_result(app.cancel_project_job(params.project_path, params.job_id))
        }
        "resume_ocr_job" => {
            let params: ResumeOcrParams = parse(value)?;
            controller.start()?;
            let event_sender = sender.clone();
            let paused_sender = sender.clone();
            let result = app.resume_ocr_job(
                params.project_path,
                params.job_id,
                &params.config,
                || {
                    controller.wait_until_runnable(|| {
                        emit(&paused_sender, "ocr-control-state", "paused");
                    })
                },
                move |progress| emit(&event_sender, "ocr-progress", progress),
            );
            controller.finish();
            serialize_result(result)
        }
        "resume_translation_job" => {
            let params: ResumeTranslationParams = parse(value)?;
            let event_sender = sender.clone();
            serialize_result(app.resume_translation_job(
                params.project_path,
                params.job_id,
                &params.config,
                || true,
                move |progress| emit(&event_sender, "translation-progress", progress),
            ))
        }
        "pause_ocr" => {
            controller.pause()?;
            serialize(())
        }
        "resume_ocr" => {
            controller.resume()?;
            serialize(())
        }
        "stop_ocr" => {
            controller.stop()?;
            serialize(())
        }
        "configure_diagnostics" => {
            let params: ConfigureDiagnosticsParams = parse(value)?;
            rosettacue_diagnostics::configure(params.enabled);
            if params.enabled {
                rosettacue_diagnostics::emit(DiagnosticEvent {
                    level: DiagnosticLevel::Info,
                    source: "backend",
                    category: "diagnostics",
                    operation: "configure",
                    phase: "enabled",
                    message: "Backend debug logging enabled.",
                    duration_ms: None,
                    details: json!({}),
                });
            }
            serialize(())
        }
        _ => Err(format!("unknown backend method: {method}")),
    }
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("invalid command parameters: {error}"))
}

fn serialize<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn serialize_result<T: Serialize, E: std::fmt::Display>(
    result: Result<T, E>,
) -> Result<Value, String> {
    serialize(result.map_err(|error| error.to_string())?)
}

fn emit<T: Serialize>(sender: &MessageSender, event: &str, payload: T) {
    send_serializable(sender, &RpcEvent { event, payload });
}

fn send_serializable<T: Serialize>(sender: &MessageSender, message: &T) {
    if let Ok(value) = serde_json::to_value(message) {
        send_value(sender, value);
    }
}

fn send_value(sender: &MessageSender, message: Value) {
    let _ = sender.send(message);
}

fn write_messages(receiver: mpsc::Receiver<Value>) {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    for message in receiver {
        if serde_json::to_writer(&mut output, &message).is_err() {
            break;
        }
        if output.write_all(b"\n").is_err() || output.flush().is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn pauses_resumes_and_stops_at_control_points() {
        let controller = OcrJobController::default();
        controller.start().expect("start OCR control");
        controller.pause().expect("pause OCR control");

        let worker_controller = controller.clone();
        let (paused_sender, paused_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let should_continue = worker_controller.wait_until_runnable(|| {
                paused_sender.send(()).expect("announce paused state");
            });
            result_sender
                .send(should_continue)
                .expect("send control result");
        });

        paused_receiver.recv().expect("wait for paused state");
        assert!(result_receiver.try_recv().is_err());
        controller.resume().expect("resume OCR control");
        assert!(result_receiver.recv().expect("continued result"));
        worker.join().expect("join control worker");

        controller.pause().expect("pause before stop");
        controller.stop().expect("stop OCR control");
        assert!(!controller.wait_until_runnable(|| {}));
        controller.finish();
    }

    #[test]
    fn parses_camel_case_command_parameters() {
        let params = parse::<CreateProjectParams>(json!({
            "parent": "/tmp",
            "name": "Example"
        }))
        .expect("parse command parameters");
        assert_eq!(params.name, "Example");
    }
}
