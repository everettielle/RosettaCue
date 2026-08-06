export type BackendMethod =
  | "backend_info"
  | "media_tool_diagnostics"
  | "create_project"
  | "save_project_as"
  | "open_project"
  | "project_document"
  | "export_subtitles"
  | "inspect_bluray_source"
  | "attach_bluray_source"
  | "extract_pgs_track"
  | "cue_image"
  | "save_cue_edit"
  | "restore_cue_edit"
  | "cue_revision_history"
  | "restore_cue_revision"
  | "delete_cue_revision"
  | "review_cue"
  | "lmstudio_models"
  | "provider_models"
  | "diagnose_provider"
  | "recognize_lmstudio"
  | "recognize_ocr"
  | "translate_cues"
  | "project_jobs"
  | "cancel_project_job"
  | "resume_ocr_job"
  | "resume_translation_job"
  | "pause_ocr"
  | "resume_ocr"
  | "stop_ocr"

export type BackendEvent =
  | "pgs-extraction-progress"
  | "ocr-progress"
  | "ocr-control-state"
  | "translation-progress"
  | "debug-log"

export type WindowMode = "welcome" | "workspace"

export type RosettaCueDesktopApi = {
  platform: string
  invoke(method: BackendMethod, params?: unknown): Promise<unknown>
  on(event: BackendEvent, listener: (payload: unknown) => void): () => void
  dialogs: {
    selectDirectory(options?: {
      title?: string
      defaultPath?: string
    }): Promise<string | null>
    selectProject(): Promise<string | null>
  }
  window: {
    setMode(mode: WindowMode): Promise<void>
  }
}

declare global {
  interface Window {
    rosettaCue?: RosettaCueDesktopApi
  }
}
