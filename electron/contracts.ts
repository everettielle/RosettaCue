export const backendMethods = [
  "backend_info",
  "media_tool_diagnostics",
  "create_project",
  "save_project_as",
  "open_project",
  "project_document",
  "export_subtitles",
  "inspect_bluray_source",
  "attach_bluray_source",
  "extract_pgs_track",
  "cue_image",
  "save_cue_edit",
  "restore_cue_edit",
  "cue_revision_history",
  "restore_cue_revision",
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
] as const

export type BackendMethod = (typeof backendMethods)[number]

export const backendEvents = [
  "pgs-extraction-progress",
  "ocr-progress",
  "ocr-control-state",
  "translation-progress",
] as const

export type BackendEvent = (typeof backendEvents)[number]

export const windowModes = ["welcome", "workspace"] as const

export type WindowMode = (typeof windowModes)[number]
