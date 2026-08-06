export type BackendInfo = {
  name: string
  version: string
  project_schema_version: number
}

export type ProjectOverview = {
  path: string
  metadata: {
    id: string
    name: string
    schema_version: number
    updated_at: string
  }
  statistics: {
    source_count: number
    track_count: number
    cue_count: number
    ocr_completed_count: number
    reviewed_count: number
  }
}

export type BlurayTitleInfo = {
  index: number
  playlist: string
  duration_seconds: number
  chapters: number
  angles: number
  clips: number
  video_tracks: number
  audio_tracks: number
  pgs_tracks: number
  pgs_languages: string[]
}

export type BlurayDiscInfo = {
  root_path: string
  display_name: string
  main_title_index: number
  titles: BlurayTitleInfo[]
}

export type ProjectSource = {
  id: string
  kind: "bluray_directory"
  display_name: string
  path: string
  fingerprint: string | null
  metadata: { type: "bluray"; data: BlurayDiscInfo }
  created_at: string
}

export type SubtitleTrack = {
  id: string
  source_id: string
  stream_index: number
  language: string | null
  codec: string
  metadata: {
    type: "pgs"
    data: { title_index: number; playlist: string; sup_path: string }
  }
}

export type SubtitlePosition =
  | "top-left"
  | "top-center"
  | "top-right"
  | "middle-left"
  | "middle-center"
  | "middle-right"
  | "bottom-left"
  | "bottom-center"
  | "bottom-right"

export type TextStyle =
  | "bold"
  | "italic"
  | "underline"
  | "strikethrough"
  | "superscript"
  | "subscript"

export type OcrSpan =
  | {
      type: "text"
      text: string
      styles: TextStyle[]
      color?: string | null
    }
  | {
      type: "ruby"
      base: string
      annotations: Array<{ text: string; position: "over" | "under" }>
      styles: TextStyle[]
      color?: string | null
    }

export type OcrLine = {
  text: string
  spans: OcrSpan[]
}

export type OcrDocument = {
  prompt_version: string
  provider: string
  model: string
  language: string
  unreadable: boolean
  lines: OcrLine[]
  normalizations: unknown[]
}

export type SubtitleCue = {
  id: string
  track_id: string
  cue_index: number
  start_ms: number
  end_ms: number
  image_path: string
  image_sha256: string
  position: SubtitlePosition
  geometry: {
    canvas_width: number
    canvas_height: number
    x: number
    y: number
    width: number
    height: number
    image_width: number
    image_height: number
    forced: boolean
    inferred_end: boolean
  }
  ocr_status: string
  review_status: string
}

export type CueRecognition = {
  cue_id: string
  document: OcrDocument
  created_at: string
}

export type CueRevision = {
  id: string
  cue_id: string
  author: "ocr" | "human" | "translation"
  document: {
    start_ms: number
    end_ms: number
    position: SubtitlePosition
    subtitle: OcrDocument
  }
  created_at: string
}

export type CueReviewDecision = {
  id: string
  cue_id: string
  revision_id: string | null
  status: "unreviewed" | "needs_review" | "approved"
  note: string
  created_at: string
}

export type SourceImportResult = {
  source: ProjectSource
  project: ProjectOverview
}

export type PgsExtractionProgress = {
  phase: string
  current: number
  estimated_total: number | null
  cue: SubtitleCue | null
}

export type PgsExtractionResult = {
  track: SubtitleTrack
  cue_count: number
  project: ProjectOverview
}

export type LlmProvider = "lm_studio" | "ollama" | "open_ai" | "anthropic"

export type ProviderConfig = {
  provider: LlmProvider
  base_url: string
  model: string
  api_key: string | null
  timeout_seconds: number
  max_tokens: number
  max_attempts: number
}

export type OcrPipelineConfig = {
  recognition: ProviderConfig
  ruby: ProviderConfig | null
  validation: ProviderConfig
}

export type LlmModel = { id: string }

export type ProviderDiagnostic = {
  provider: LlmProvider
  reachable: boolean
  latency_ms: number
  models: LlmModel[]
  message: string
}

export type MediaToolDiagnostic = {
  name: string
  required_for: "source_analysis" | "pgs_extraction"
  available: boolean
  path: string | null
  origin: "configured" | "bundled" | "path" | "system" | null
  version: string | null
  message: string
}

export type OcrProgress = {
  phase: string
  current: number
  total: number
  cue_id: string | null
  cue_index: number | null
  recognition: CueRecognition | null
  error: string | null
}

export type OcrJobResult = {
  job_id: string | null
  processed: number
  project: ProjectOverview
}

export type TranslationProgress = {
  phase: string
  current: number
  total: number
  cue_id: string | null
  cue_index: number | null
  revision: CueRevision | null
  error: string | null
}

export type TranslationJobResult = {
  job_id: string | null
  processed: number
  project: ProjectOverview
}

export type ExportFormat = "json" | "srt"
export type ExportScope = "all_recognized" | "approved_only"

export type ExportOptions = {
  track_id: string | null
  formats: ExportFormat[]
  scope: ExportScope
  output_directory: string
  base_name: string | null
}

export type ExportResult = {
  track_id: string
  artifacts: Array<{
    format: ExportFormat
    path: string
    cue_count: number
    warnings: string[]
  }>
  skipped_cues: number
}

export type ProjectDocument = {
  project: ProjectOverview
  sources: ProjectSource[]
  tracks: SubtitleTrack[]
  cues: SubtitleCue[]
  recognitions: CueRecognition[]
  revisions: CueRevision[]
  review_decisions: CueReviewDecision[]
}
