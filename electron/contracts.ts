export const backendMethods = [
  "backend_info",
  "media_tool_diagnostics",
  "create_project",
  "save_project_as",
  "open_project",
  "project_document",
  "update_project_settings",
  "export_subtitles",
  "inspect_bluray_source",
  "attach_bluray_source",
  "extract_pgs_track",
  "cue_image",
  "save_cue_edit",
  "restore_cue_edit",
  "cue_revision_history",
  "restore_cue_revision",
  "delete_cue_revision",
  "review_cue",
  "lmstudio_models",
  "provider_models",
  "diagnose_provider",
  "recognize_lmstudio",
  "recognize_ocr",
  "translate_cues",
  "project_jobs",
  "cancel_project_job",
  "resume_ocr_job",
  "resume_translation_job",
  "pause_ocr",
  "resume_ocr",
  "stop_ocr",
  "configure_diagnostics",
] as const

export type BackendMethod = (typeof backendMethods)[number]

export const backendEvents = [
  "pgs-extraction-progress",
  "ocr-progress",
  "ocr-control-state",
  "translation-progress",
  "diagnostic-log",
] as const

export type BackendEvent = (typeof backendEvents)[number]

export const windowModes = ["welcome", "workspace"] as const

export type WindowMode = (typeof windowModes)[number]

export type DiagnosticLevel = "debug" | "info" | "warn" | "error"

export type DiagnosticEntry = {
  id: string
  session_id: string
  sequence: number
  created_at_ms: number
  level: DiagnosticLevel
  source: string
  category: string
  operation: string
  phase: string
  message: string
  correlation_id: string | null
  duration_ms: number | null
  details: unknown
}

export type DiagnosticSnapshot = {
  enabled: boolean
  session_id: string
  current_session_id: string
  sessions: string[]
  entries: DiagnosticEntry[]
}
