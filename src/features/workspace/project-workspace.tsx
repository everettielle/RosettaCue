import * as React from "react"
import type { LucideIcon } from "lucide-react"
import { usePanelRef } from "react-resizable-panels"
import {
  CheckCircle2Icon,
  ChevronLeftIcon,
  ChevronRightIcon,
  DownloadIcon,
  FileOutputIcon,
  FolderInputIcon,
  HistoryIcon,
  LanguagesIcon,
  ListChecksIcon,
  PauseIcon,
  PlayIcon,
  Redo2Icon,
  RefreshCwIcon,
  SaveIcon,
  SearchIcon,
  Settings2Icon,
  SparklesIcon,
  SquareIcon,
  Undo2Icon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Spinner } from "@/components/ui/spinner"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toast } from "@/components/ui/toast"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import type {
  CueRecognition,
  CueRevision,
  ExportOptions,
  ExportResult,
  OcrDocument,
  OcrDebugLogEntry,
  OcrJobResult,
  OcrLine,
  OcrProgress,
  PgsExtractionProgress,
  PgsExtractionResult,
  ProjectDocument,
  ProjectOverview,
  SourceImportResult,
  SubtitleCue,
  SubtitlePosition,
  TranslationJobResult,
  TranslationProgress,
} from "@/features/projects/types"
import {
  loadWorkspaceSettings,
  saveWorkspaceSettings,
  validateProviderConfig,
  type WorkspaceSettings,
} from "@/features/settings/model-settings"
import { DebugLogDialog } from "@/features/settings/debug-log-dialog"
import { SettingsDialog } from "@/features/settings/settings-dialog"
import { RevisionHistoryDialog } from "@/features/workspace/revision-history-dialog"
import {
  ExportDialog,
  PgsExtractionDialog,
  SaveProjectAsDialog,
  SourceImportDialog,
} from "@/features/workspace/workspace-dialogs"
import {
  cueImagePlacement,
  cueSubtitlePlacement,
} from "@/features/workspace/cue-geometry"
import { SubtitleContentEditor } from "@/features/workspace/subtitle-content-editor"
import { RenderedSubtitle } from "@/features/workspace/rendered-subtitle"
import { normalizeOcrLines } from "@/features/workspace/subtitle-spans"
import { desktop } from "@/lib/desktop"
import { cn } from "@/lib/utils"
import * as m from "@/paraglide/messages.js"
import { getLocale } from "@/paraglide/runtime.js"

type RibbonCommand = {
  id:
    | "save"
    | "save-as"
    | "import-source"
    | "export"
    | "undo"
    | "redo"
    | "extract-pgs"
    | "start-ocr"
    | "pause-ocr"
    | "resume-ocr"
    | "stop-ocr"
    | "ocr-cue"
    | "approve"
    | "approve-next"
    | "history"
    | "previous-cue"
    | "next-cue"
    | "refresh"
    | "translate-cue"
    | "translate-all"
  label: string
  icon: LucideIcon
}

const INSPECTOR_MIN_HEIGHT = 220
const WORKSPACE_ERROR_TOAST_ID = "workspace-error"

const ribbonTabs: Array<{
  value: string
  label: string
  commands: RibbonCommand[]
}> = [
  {
    value: "home",
    label: m.workspace_tab_home(),
    commands: [
      { id: "save", label: m.workspace_save(), icon: SaveIcon },
      { id: "undo", label: m.workspace_undo(), icon: Undo2Icon },
      { id: "redo", label: m.workspace_redo(), icon: Redo2Icon },
      { id: "ocr-cue", label: m.workspace_ocr_cue(), icon: SparklesIcon },
      {
        id: "translate-cue",
        label: m.workspace_translate_cue(),
        icon: LanguagesIcon,
      },
      {
        id: "approve-next",
        label: m.workspace_review_and_next(),
        icon: ListChecksIcon,
      },
      {
        id: "history",
        label: m.revision_history_title(),
        icon: HistoryIcon,
      },
    ],
  },
  {
    value: "project",
    label: m.workspace_tab_project(),
    commands: [
      { id: "save", label: m.workspace_save(), icon: SaveIcon },
      { id: "save-as", label: m.workspace_save_as(), icon: FileOutputIcon },
      {
        id: "import-source",
        label: m.workspace_import_source(),
        icon: FolderInputIcon,
      },
      { id: "export", label: m.workspace_export(), icon: DownloadIcon },
    ],
  },
  {
    value: "edit",
    label: m.workspace_tab_edit(),
    commands: [
      { id: "undo", label: m.workspace_undo(), icon: Undo2Icon },
      { id: "redo", label: m.workspace_redo(), icon: Redo2Icon },
    ],
  },
  {
    value: "subtitle",
    label: m.workspace_tab_subtitle(),
    commands: [
      { id: "extract-pgs", label: m.pgs_extract_track(), icon: SparklesIcon },
      { id: "start-ocr", label: m.workspace_start_full_ocr(), icon: PlayIcon },
      { id: "pause-ocr", label: m.workspace_pause(), icon: PauseIcon },
      { id: "resume-ocr", label: m.workspace_resume(), icon: PlayIcon },
      { id: "stop-ocr", label: m.workspace_stop(), icon: SquareIcon },
    ],
  },
  {
    value: "review",
    label: m.workspace_tab_review(),
    commands: [
      {
        id: "approve",
        label: m.workspace_mark_reviewed(),
        icon: CheckCircle2Icon,
      },
      {
        id: "approve-next",
        label: m.workspace_review_and_next(),
        icon: ListChecksIcon,
      },
      {
        id: "history",
        label: m.revision_history_title(),
        icon: HistoryIcon,
      },
      {
        id: "previous-cue",
        label: m.workspace_previous_cue(),
        icon: ChevronLeftIcon,
      },
      {
        id: "next-cue",
        label: m.workspace_next_cue(),
        icon: ChevronRightIcon,
      },
      {
        id: "refresh",
        label: m.workspace_refresh_cues(),
        icon: RefreshCwIcon,
      },
    ],
  },
  {
    value: "translate",
    label: m.workspace_tab_translate(),
    commands: [
      {
        id: "translate-cue",
        label: m.workspace_translate_cue(),
        icon: LanguagesIcon,
      },
      {
        id: "translate-all",
        label: m.workspace_translate_all(),
        icon: PlayIcon,
      },
    ],
  },
]

type OcrControlState = "idle" | "running" | "paused" | "stopping"

type CueEditDraft = {
  lines: OcrLine[]
  start: string
  end: string
  position: SubtitlePosition
}

const subtitlePositions: SubtitlePosition[] = [
  "top-left",
  "top-center",
  "top-right",
  "middle-left",
  "middle-center",
  "middle-right",
  "bottom-left",
  "bottom-center",
  "bottom-right",
]

function subtitlePositionLabel(value: SubtitlePosition) {
  const labels: Record<SubtitlePosition, () => string> = {
    "top-left": m.workspace_position_top_left,
    "top-center": m.workspace_position_top_center,
    "top-right": m.workspace_position_top_right,
    "middle-left": m.workspace_position_middle_left,
    "middle-center": m.workspace_position_middle_center,
    "middle-right": m.workspace_position_middle_right,
    "bottom-left": m.workspace_position_bottom_left,
    "bottom-center": m.workspace_position_bottom_center,
    "bottom-right": m.workspace_position_bottom_right,
  }
  return labels[value]()
}

function ocrStatusLabel(value: string) {
  const labels: Record<string, () => string> = {
    pending: m.status_ocr_pending,
    running: m.status_ocr_running,
    succeeded: m.status_ocr_succeeded,
    failed: m.status_ocr_failed,
  }
  return labels[value]?.() ?? value
}

function reviewStatusLabel(value: string) {
  const labels: Record<string, () => string> = {
    unreviewed: m.status_review_unreviewed,
    needs_review: m.status_review_needs_review,
    approved: m.status_review_approved,
  }
  return labels[value]?.() ?? value
}

function ocrControlStateLabel(value: OcrControlState) {
  const labels: Record<OcrControlState, () => string> = {
    idle: m.workspace_ocr_state_idle,
    running: m.workspace_ocr_state_running,
    paused: m.workspace_ocr_state_paused,
    stopping: m.workspace_ocr_state_stopping,
  }
  return labels[value]()
}

function formatTimestamp(value: number) {
  const hours = Math.floor(value / 3_600_000)
  const minutes = Math.floor((value % 3_600_000) / 60_000)
  const seconds = Math.floor((value % 60_000) / 1_000)
  const milliseconds = value % 1_000
  return [hours, minutes, seconds]
    .map((part) => String(part).padStart(2, "0"))
    .join(":")
    .concat(`.${String(milliseconds).padStart(3, "0")}`)
}

function parseTimestamp(value: string) {
  const match = /^(\d+):([0-5]\d):([0-5]\d)\.(\d{3})$/.exec(value.trim())
  if (!match) return null
  return (
    Number(match[1]) * 3_600_000 +
    Number(match[2]) * 60_000 +
    Number(match[3]) * 1_000 +
    Number(match[4])
  )
}

function plainText(document: OcrDocument | null) {
  return document?.lines.map((line) => line.text).join("\n") ?? ""
}

function latestRevisionForCue(revisions: CueRevision[], cueId: string) {
  return revisions
    .filter((revision) => revision.cue_id === cueId)
    .sort((left, right) => left.created_at.localeCompare(right.created_at))
    .at(-1)
}

function recognitionForCue(recognitions: CueRecognition[], cueId: string) {
  return recognitions
    .filter((recognition) => recognition.cue_id === cueId)
    .sort((left, right) => left.created_at.localeCompare(right.created_at))
    .at(-1)
}

function documentForCue(project: ProjectDocument, cueId: string) {
  return (
    latestRevisionForCue(project.revisions, cueId)?.document.subtitle ??
    recognitionForCue(project.recognitions, cueId)?.document ??
    null
  )
}

function IconButton({
  label,
  icon: Icon,
  onClick,
  disabled,
  size = "icon-sm",
}: {
  label: string
  icon: LucideIcon
  onClick: () => void
  disabled?: boolean
  size?: "icon-xs" | "icon-sm"
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size={size}
            aria-label={label}
            onClick={onClick}
            disabled={disabled}
          />
        }
      >
        <Icon />
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

export function ProjectWorkspace({
  project,
  onProjectChange,
}: {
  project: ProjectOverview
  onProjectChange: (project: ProjectOverview) => void
}) {
  const [document, setDocument] = React.useState<ProjectDocument | null>(null)
  const [activeCueId, setActiveCueId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState("")
  const [draftByCue, setDraftByCue] = React.useState<
    Record<string, CueEditDraft>
  >({})
  const [loading, setLoading] = React.useState(true)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [statusMessage, setStatusMessage] = React.useState<string>(
    m.status_ready()
  )
  const [settings, setSettings] = React.useState<WorkspaceSettings>(
    loadWorkspaceSettings
  )
  const [settingsOpen, setSettingsOpen] = React.useState(false)
  const [debugLogOpen, setDebugLogOpen] = React.useState(false)
  const [debugLogs, setDebugLogs] = React.useState<OcrDebugLogEntry[]>([])
  const [saveAsOpen, setSaveAsOpen] = React.useState(false)
  const [sourceImportOpen, setSourceImportOpen] = React.useState(false)
  const [pgsOpen, setPgsOpen] = React.useState(false)
  const [exportOpen, setExportOpen] = React.useState(false)
  const [revisionHistoryOpen, setRevisionHistoryOpen] = React.useState(false)
  const [pgsProgress, setPgsProgress] =
    React.useState<PgsExtractionProgress | null>(null)
  const [pgsBusy, setPgsBusy] = React.useState(false)
  const [pgsError, setPgsError] = React.useState<string | null>(null)
  const [ocrState, setOcrState] = React.useState<OcrControlState>("idle")
  const [ocrProgress, setOcrProgress] = React.useState<OcrProgress | null>(null)
  const [translationProgress, setTranslationProgress] =
    React.useState<TranslationProgress | null>(null)
  const [translationBusy, setTranslationBusy] = React.useState(false)
  const [redoByCue, setRedoByCue] = React.useState<Record<string, string[]>>({})
  const [inspectorContentHeight, setInspectorContentHeight] = React.useState<
    number | null
  >(null)
  const inspectorPanelRef = usePanelRef()
  const hasLoadedDocumentRef = React.useRef(false)
  const inspectorMaxHeight =
    inspectorContentHeight === null
      ? undefined
      : Math.max(INSPECTOR_MIN_HEIGHT, inspectorContentHeight)

  const handleInspectorContentHeightChange = React.useCallback(
    (height: number) => {
      setInspectorContentHeight((current) =>
        current === height ? current : height
      )
    },
    []
  )

  React.useLayoutEffect(() => {
    if (inspectorMaxHeight === undefined) return

    const panel = inspectorPanelRef.current
    if (panel && panel.getSize().inPixels > inspectorMaxHeight) {
      panel.resize(inspectorMaxHeight)
    }
  }, [inspectorMaxHeight, inspectorPanelRef])

  React.useEffect(() => {
    if (!error) {
      toast.close(WORKSPACE_ERROR_TOAST_ID)
      return
    }

    toast.add({
      id: WORKSPACE_ERROR_TOAST_ID,
      title: m.workspace_action_failed(),
      description: error,
      type: "error",
      timeout: 0,
      priority: "high",
      onClose: () => setError(null),
    })
  }, [error])

  React.useEffect(() => () => toast.close(WORKSPACE_ERROR_TOAST_ID), [])

  const loadDocument = React.useCallback(async () => {
    const showLoading = !hasLoadedDocumentRef.current
    if (showLoading) setLoading(true)
    setError(null)
    try {
      const next = await desktop.invoke<ProjectDocument>("project_document", {
        path: project.path,
      })
      setDocument(next)
      hasLoadedDocumentRef.current = true
      setActiveCueId((current) =>
        current && next.cues.some((cue) => cue.id === current)
          ? current
          : (next.cues[0]?.id ?? null)
      )
      setStatusMessage(m.status_project_loaded())
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage(m.status_unable_load_project())
    } finally {
      if (showLoading) setLoading(false)
    }
  }, [project.path])

  React.useEffect(() => {
    let active = true
    hasLoadedDocumentRef.current = false
    desktop
      .invoke<ProjectDocument>("project_document", { path: project.path })
      .then((next) => {
        if (!active) {
          return
        }
        setDocument(next)
        hasLoadedDocumentRef.current = true
        setActiveCueId(next.cues[0]?.id ?? null)
        setStatusMessage(m.status_project_loaded())
      })
      .catch((reason: unknown) => {
        if (!active) {
          return
        }
        setError(reason instanceof Error ? reason.message : String(reason))
        setStatusMessage(m.status_unable_load_project())
      })
      .finally(() => {
        if (active) {
          setLoading(false)
        }
      })
    return () => {
      active = false
    }
  }, [project.path])

  React.useEffect(() => {
    const removePgsListener = desktop.on<PgsExtractionProgress>(
      "pgs-extraction-progress",
      (progress) => {
        setPgsProgress(progress)
        const extractedCue = progress.cue
        if (extractedCue) {
          setDocument((current) => {
            if (
              !current ||
              current.cues.some((cue) => cue.id === extractedCue.id)
            ) {
              return current
            }
            return {
              ...current,
              cues: [...current.cues, extractedCue].sort(
                (left, right) => left.cue_index - right.cue_index
              ),
            }
          })
        }
      }
    )
    const removeOcrListener = desktop.on<OcrProgress>(
      "ocr-progress",
      (progress) => {
        setOcrProgress(progress)
        if (progress.phase === "completed" || progress.phase === "stopped") {
          setOcrState("idle")
        }
        if (progress.recognition && progress.cue_id) {
          const recognition = progress.recognition
          setDocument((current) => {
            if (!current) return current
            const cue = current.cues.find(
              (candidate) => candidate.id === progress.cue_id
            )
            if (!cue) return current
            const recognitionList = current.recognitions.filter(
              (candidate) => candidate.cue_id !== recognition.cue_id
            )
            const revision: CueRevision = {
              id: `live-${recognition.cue_id}-${recognition.created_at}`,
              cue_id: recognition.cue_id,
              author: "ocr",
              document: {
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                position: cue.position,
                subtitle: recognition.document,
              },
              created_at: recognition.created_at,
            }
            return {
              ...current,
              cues: current.cues.map((candidate) =>
                candidate.id === recognition.cue_id
                  ? { ...candidate, ocr_status: "succeeded" }
                  : candidate
              ),
              recognitions: [...recognitionList, recognition],
              revisions: [
                ...current.revisions.filter(
                  (candidate) => candidate.cue_id !== recognition.cue_id
                ),
                revision,
              ],
              project: {
                ...current.project,
                statistics: {
                  ...current.project.statistics,
                  ocr_completed_count: Math.max(
                    current.project.statistics.ocr_completed_count,
                    current.cues.filter(
                      (candidate) =>
                        candidate.ocr_status === "succeeded" ||
                        candidate.id === recognition.cue_id
                    ).length
                  ),
                },
              },
            }
          })
        }
      }
    )
    const removeControlListener = desktop.on<string>(
      "ocr-control-state",
      (state) => {
        if (state === "paused") setOcrState("paused")
      }
    )
    const removeTranslationListener = desktop.on<TranslationProgress>(
      "translation-progress",
      (progress) => {
        setTranslationProgress(progress)
        if (progress.revision) {
          const revision = progress.revision
          setDocument((current) =>
            current
              ? {
                  ...current,
                  revisions: [
                    ...current.revisions.filter(
                      (candidate) => candidate.cue_id !== revision.cue_id
                    ),
                    revision,
                  ],
                }
              : current
          )
        }
      }
    )
    const removeDebugListener = desktop.on<OcrDebugLogEntry>(
      "debug-log",
      (entry) => {
        setDebugLogs((current) => [...current.slice(-199), entry])
      }
    )
    return () => {
      removePgsListener()
      removeOcrListener()
      removeControlListener()
      removeTranslationListener()
      removeDebugListener()
    }
  }, [])

  const selectedCue = React.useMemo(
    () => document?.cues.find((cue) => cue.id === activeCueId) ?? null,
    [activeCueId, document]
  )
  const selectedDocument = React.useMemo(
    () =>
      document && activeCueId ? documentForCue(document, activeCueId) : null,
    [activeCueId, document]
  )
  const selectedRevision = React.useMemo(
    () =>
      document && activeCueId
        ? latestRevisionForCue(document.revisions, activeCueId)
        : undefined,
    [activeCueId, document]
  )
  const baseStart =
    selectedRevision?.document.start_ms ?? selectedCue?.start_ms ?? 0
  const baseEnd = selectedRevision?.document.end_ms ?? selectedCue?.end_ms ?? 0
  const basePosition =
    selectedRevision?.document.position ??
    selectedCue?.position ??
    "bottom-center"
  const cueDraft = React.useMemo<CueEditDraft | null>(
    () =>
      activeCueId && selectedCue
        ? (draftByCue[activeCueId] ?? {
            lines: selectedDocument?.lines ?? [],
            start: formatTimestamp(baseStart),
            end: formatTimestamp(baseEnd),
            position: basePosition,
          })
        : null,
    [
      activeCueId,
      baseEnd,
      basePosition,
      baseStart,
      draftByCue,
      selectedCue,
      selectedDocument,
    ]
  )
  const previewDocument = React.useMemo(() => {
    if (!selectedDocument || !cueDraft) return selectedDocument
    return { ...selectedDocument, lines: cueDraft.lines }
  }, [cueDraft, selectedDocument])
  const draftChanged = Boolean(
    cueDraft &&
    selectedDocument &&
    (JSON.stringify(cueDraft.lines) !==
      JSON.stringify(selectedDocument.lines) ||
      cueDraft.start !== formatTimestamp(baseStart) ||
      cueDraft.end !== formatTimestamp(baseEnd) ||
      cueDraft.position !== basePosition)
  )

  const updateCueDraft = (patch: Partial<CueEditDraft>) => {
    if (!activeCueId || !cueDraft) return
    setDraftByCue((current) => ({
      ...current,
      [activeCueId]: { ...cueDraft, ...patch },
    }))
  }

  const cueText = React.useMemo(() => {
    const textByCue = new Map<string, string>()
    if (document) {
      for (const cue of document.cues) {
        textByCue.set(cue.id, plainText(documentForCue(document, cue.id)))
      }
    }
    return textByCue
  }, [document])

  const filteredCues = React.useMemo(() => {
    if (!document) {
      return []
    }
    const normalizedQuery = query.trim().toLocaleLowerCase()
    if (!normalizedQuery) {
      return document.cues
    }
    return document.cues.filter((cue) => {
      const cueNumber = String(cue.cue_index).padStart(6, "0")
      const text = cueText.get(cue.id)?.toLocaleLowerCase() ?? ""
      return (
        cueNumber.includes(normalizedQuery) || text.includes(normalizedQuery)
      )
    })
  }, [cueText, document, query])

  const selectedIndex = selectedCue
    ? filteredCues.findIndex((cue) => cue.id === selectedCue.id)
    : -1
  const statistics = document?.project.statistics ?? project.statistics

  const selectAdjacentCue = (offset: number) => {
    const nextCue = filteredCues[selectedIndex + offset]
    if (nextCue) {
      setActiveCueId(nextCue.id)
    }
  }

  const saveCue = async () => {
    if (!document || !selectedCue || !selectedDocument || !cueDraft) {
      return
    }
    const startMs = parseTimestamp(cueDraft.start)
    const endMs = parseTimestamp(cueDraft.end)
    if (startMs === null || endMs === null || endMs <= startMs) {
      setError(m.workspace_timestamp_error())
      return
    }
    setSaving(true)
    setError(null)
    try {
      await desktop.invoke("save_cue_edit", {
        projectPath: project.path,
        cueId: selectedCue.id,
        document: {
          start_ms: startMs,
          end_ms: endMs,
          position: cueDraft.position,
          subtitle: {
            ...selectedDocument,
            lines: normalizeOcrLines(cueDraft.lines),
          },
        },
      })
      await loadDocument()
      setDraftByCue((current) => {
        const next = { ...current }
        delete next[selectedCue.id]
        return next
      })
      setStatusMessage(m.status_cue_saved({ index: selectedCue.cue_index }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage(m.status_unable_save_cue())
    } finally {
      setSaving(false)
    }
  }

  const requireModel = (task: "ocr" | "validation" | "translation") => {
    const message = validateProviderConfig(settings.profiles[task])
    if (message) {
      setError(message)
      setSettingsOpen(true)
      setStatusMessage(m.status_configure_model())
      return false
    }
    return true
  }

  const runOcr = async (cueIds?: string[]) => {
    if (!requireModel("ocr") || !requireModel("validation")) return
    setError(null)
    setOcrState("running")
    setOcrProgress(null)
    setStatusMessage(cueIds ? m.status_ocr_selected() : m.status_ocr_started())
    try {
      const result = await desktop.invoke<OcrJobResult>("recognize_ocr", {
        projectPath: project.path,
        cueIds: cueIds ?? null,
        language: settings.ocr_language,
        overwrite: Boolean(cueIds),
        config: {
          recognition: settings.profiles.ocr,
          validation: settings.profiles.validation,
          debug_logging: settings.debug_logging,
        },
      })
      onProjectChange(result.project)
      await loadDocument()
      setStatusMessage(m.status_ocr_completed({ count: result.processed }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage(m.status_ocr_error())
    } finally {
      setOcrState("idle")
    }
  }

  const controlOcr = async (action: "pause" | "resume" | "stop") => {
    setError(null)
    try {
      await desktop.invoke(`${action}_ocr`)
      const nextState: OcrControlState =
        action === "pause"
          ? "paused"
          : action === "resume"
            ? "running"
            : "stopping"
      setOcrState(nextState)
      setStatusMessage(
        action === "pause"
          ? m.status_ocr_pause_pending()
          : action === "resume"
            ? m.status_ocr_resumed()
            : m.status_ocr_stop_pending()
      )
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }

  const translate = async (cueIds?: string[]) => {
    if (!requireModel("translation")) return
    setTranslationBusy(true)
    setTranslationProgress(null)
    setError(null)
    setStatusMessage(
      cueIds ? m.status_translation_selected() : m.status_translation_started()
    )
    try {
      const result = await desktop.invoke<TranslationJobResult>(
        "translate_cues",
        {
          projectPath: project.path,
          cueIds: cueIds ?? null,
          targetLanguage: settings.target_language,
          overwrite: Boolean(cueIds),
          config: settings.profiles.translation,
        }
      )
      onProjectChange(result.project)
      await loadDocument()
      setStatusMessage(
        m.status_translation_completed({ count: result.processed })
      )
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage(m.status_translation_error())
    } finally {
      setTranslationBusy(false)
    }
  }

  const reviewSelectedCue = async (moveToNext = false) => {
    if (!selectedCue || !selectedDocument) return
    setError(null)
    try {
      const result = await desktop.invoke<{ project: ProjectOverview }>(
        "review_cue",
        {
          projectPath: project.path,
          cueId: selectedCue.id,
          status: "approved",
          note: "",
        }
      )
      onProjectChange(result.project)
      await loadDocument()
      setStatusMessage(m.status_cue_reviewed({ index: selectedCue.cue_index }))
      if (moveToNext) selectAdjacentCue(1)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }

  const undoSelectedCue = async () => {
    if (!selectedCue) return
    setError(null)
    try {
      const history = await desktop.invoke<CueRevision[]>(
        "cue_revision_history",
        { projectPath: project.path, cueId: selectedCue.id }
      )
      if (history.length < 2) {
        setStatusMessage(m.status_no_earlier_revision())
        return
      }
      setRedoByCue((current) => ({
        ...current,
        [selectedCue.id]: [...(current[selectedCue.id] ?? []), history[0].id],
      }))
      await desktop.invoke("restore_cue_revision", {
        projectPath: project.path,
        cueId: selectedCue.id,
        revisionId: history[1].id,
      })
      await loadDocument()
      setStatusMessage(m.status_undid_cue({ index: selectedCue.cue_index }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }

  const redoSelectedCue = async () => {
    if (!selectedCue) return
    const stack = redoByCue[selectedCue.id] ?? []
    const revisionId = stack.at(-1)
    if (!revisionId) return
    setError(null)
    try {
      await desktop.invoke("restore_cue_revision", {
        projectPath: project.path,
        cueId: selectedCue.id,
        revisionId,
      })
      setRedoByCue((current) => ({
        ...current,
        [selectedCue.id]: (current[selectedCue.id] ?? []).slice(0, -1),
      }))
      await loadDocument()
      setStatusMessage(m.status_redid_cue({ index: selectedCue.cue_index }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }

  const extractPgs = async (
    sourceId: string,
    titleIndex: number,
    streamIndex: number
  ) => {
    setPgsBusy(true)
    setPgsProgress(null)
    setPgsError(null)
    try {
      const result = await desktop.invoke<PgsExtractionResult>(
        "extract_pgs_track",
        { projectPath: project.path, sourceId, titleIndex, streamIndex }
      )
      onProjectChange(result.project)
      await loadDocument()
      setPgsOpen(false)
      setStatusMessage(m.status_pgs_complete({ count: result.cue_count }))
    } catch (reason) {
      setPgsError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPgsBusy(false)
    }
  }

  const exportSubtitles = (options: ExportOptions) =>
    desktop.invoke<ExportResult>("export_subtitles", {
      projectPath: project.path,
      options,
    })

  const commandDisabled = (command: RibbonCommand) => {
    switch (command.id) {
      case "save":
        return saving
      case "export":
        return !document?.tracks.length
      case "undo":
        return !selectedRevision
      case "redo":
        return !selectedCue || !(redoByCue[selectedCue.id]?.length > 0)
      case "start-ocr":
        return ocrState !== "idle" || !document?.cues.length
      case "ocr-cue":
        return ocrState !== "idle" || !selectedCue
      case "pause-ocr":
        return ocrState !== "running"
      case "resume-ocr":
        return ocrState !== "paused"
      case "stop-ocr":
        return ocrState === "idle" || ocrState === "stopping"
      case "approve":
        return !selectedDocument || selectedCue?.review_status === "approved"
      case "approve-next":
        return (
          !selectedDocument ||
          selectedCue?.review_status === "approved" ||
          selectedIndex < 0 ||
          selectedIndex >= filteredCues.length - 1
        )
      case "history":
        return !selectedCue
      case "previous-cue":
        return selectedIndex <= 0
      case "next-cue":
        return selectedIndex < 0 || selectedIndex >= filteredCues.length - 1
      case "translate-cue":
        return !selectedDocument || translationBusy
      case "translate-all":
        return translationBusy || !document?.revisions.length
      default:
        return false
    }
  }

  const handleRibbonCommand = (command: RibbonCommand) => {
    switch (command.id) {
      case "save":
        if (draftChanged) void saveCue()
        else setStatusMessage(m.status_project_up_to_date())
        return
      case "save-as":
        setSaveAsOpen(true)
        return
      case "import-source":
        setSourceImportOpen(true)
        return
      case "export":
        setExportOpen(true)
        return
      case "undo":
        void undoSelectedCue()
        return
      case "redo":
        void redoSelectedCue()
        return
      case "extract-pgs":
        if (document?.sources.length) setPgsOpen(true)
        else setSourceImportOpen(true)
        return
      case "start-ocr":
        void runOcr()
        return
      case "ocr-cue":
        if (selectedCue) void runOcr([selectedCue.id])
        return
      case "pause-ocr":
        void controlOcr("pause")
        return
      case "resume-ocr":
        void controlOcr("resume")
        return
      case "stop-ocr":
        void controlOcr("stop")
        return
      case "approve":
        void reviewSelectedCue()
        return
      case "approve-next":
        void reviewSelectedCue(true)
        return
      case "history":
        setRevisionHistoryOpen(true)
        return
      case "previous-cue":
        selectAdjacentCue(-1)
        return
      case "next-cue":
        selectAdjacentCue(1)
        return
      case "refresh":
        void loadDocument()
        return
      case "translate-cue":
        if (selectedCue) void translate([selectedCue.id])
        return
      case "translate-all":
        void translate()
    }
  }

  return (
    <main className="flex h-svh min-h-0 flex-col overflow-hidden bg-background text-foreground">
      <header className="flex h-14 shrink-0 items-center border-b px-4 [-webkit-app-region:drag]">
        <div
          className={cn(
            "flex min-w-0 items-center gap-3",
            desktop.platform === "darwin" && "pl-20"
          )}
        >
          <img
            src="./rosettacue-mark.png"
            alt=""
            className="size-8 rounded-xl"
          />
          <div className="min-w-0">
            <p className="truncate text-sm font-medium">
              {project.metadata.name}
            </p>
            <p className="max-w-xl truncate text-xs text-muted-foreground">
              {project.path}
            </p>
          </div>
        </div>
      </header>

      <Tabs defaultValue="home" className="shrink-0 gap-0">
        <TabsList
          variant="line"
          className="h-9 w-full shrink-0 justify-start rounded-none border-b px-4"
        >
          {ribbonTabs.map((tab) => (
            <TabsTrigger
              key={tab.value}
              value={tab.value}
              className="flex-none px-3"
            >
              {tab.label}
            </TabsTrigger>
          ))}
        </TabsList>

        {ribbonTabs.map((tab) => (
          <TabsContent
            key={tab.value}
            value={tab.value}
            className="flex min-h-0 flex-1 flex-col"
          >
            <div className="flex h-20 shrink-0 items-center gap-1 overflow-x-auto border-b bg-muted/30 px-3">
              {tab.commands.map((command) => {
                const Icon = command.icon
                return (
                  <Button
                    key={command.id}
                    variant="ghost"
                    className="h-14 shrink-0 flex-col gap-1 px-3"
                    disabled={commandDisabled(command)}
                    onClick={() => handleRibbonCommand(command)}
                  >
                    <Icon data-icon="inline-start" />
                    <span className="text-xs">{command.label}</span>
                  </Button>
                )
              })}
              <Separator orientation="vertical" className="mx-2" />
              <div className="ml-auto flex shrink-0 items-center gap-6 px-2">
                <RibbonStatistic
                  value={statistics.cue_count}
                  label={m.workspace_stat_cues()}
                />
                <RibbonStatistic
                  value={statistics.source_count}
                  label={m.workspace_stat_sources()}
                />
                <RibbonStatistic
                  value={statistics.ocr_completed_count}
                  label={m.workspace_ocr_complete()}
                />
                <RibbonStatistic
                  value={statistics.reviewed_count}
                  label={m.workspace_stat_reviewed()}
                />
              </div>
              <Separator orientation="vertical" className="mx-2" />
              <Button
                variant="ghost"
                className="h-14 shrink-0 flex-col gap-1 px-3"
                onClick={() => setSettingsOpen(true)}
              >
                <Settings2Icon data-icon="inline-start" />
                <span className="text-xs">{m.workspace_settings()}</span>
              </Button>
            </div>
          </TabsContent>
        ))}
      </Tabs>

      <div className="min-h-0 flex-1">
        <ResizablePanelGroup orientation="horizontal">
          <ResizablePanel
            defaultSize="22%"
            minSize={180}
            maxSize="34%"
            className="min-w-0 overflow-hidden"
          >
            <section className="flex h-full min-h-0 flex-col bg-muted/20">
              <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
                <h2 className="text-sm font-medium">
                  {m.workspace_cue_list()}
                </h2>
                <Badge variant="secondary" className="ml-auto">
                  {filteredCues.length}
                </Badge>
              </div>
              <div className="border-b p-2">
                <div className="relative">
                  <SearchIcon className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    value={query}
                    className="pl-8"
                    placeholder={m.workspace_search_placeholder()}
                    aria-label={m.workspace_search_cues()}
                    onChange={(event) => setQuery(event.target.value)}
                  />
                </div>
              </div>
              <ScrollArea className="min-h-0 flex-1">
                {loading ? (
                  <div className="flex h-full items-center justify-center">
                    <Spinner />
                  </div>
                ) : filteredCues.length === 0 ? (
                  <Empty className="border-0 p-6">
                    <EmptyHeader>
                      <EmptyMedia variant="icon">
                        <SearchIcon />
                      </EmptyMedia>
                      <EmptyTitle>{m.workspace_no_cues()}</EmptyTitle>
                      <EmptyDescription>
                        {m.workspace_no_cues_description()}
                      </EmptyDescription>
                    </EmptyHeader>
                  </Empty>
                ) : (
                  <div className="flex flex-col py-1">
                    {filteredCues.map((cue) => (
                      <Button
                        key={cue.id}
                        variant={cue.id === activeCueId ? "secondary" : "ghost"}
                        className="h-auto w-full justify-start rounded-none px-3 py-2 text-left"
                        onClick={() => setActiveCueId(cue.id)}
                      >
                        <span className="flex min-w-0 flex-1 flex-col gap-1">
                          <span className="flex items-center gap-2">
                            <span className="font-mono text-xs font-medium">
                              {String(cue.cue_index).padStart(6, "0")}
                            </span>
                            <span className="ml-auto text-xs font-normal text-muted-foreground">
                              {formatTimestamp(cue.start_ms)}
                            </span>
                          </span>
                          <span className="truncate text-xs font-normal text-muted-foreground">
                            {cueText.get(cue.id) || m.workspace_waiting_ocr()}
                          </span>
                        </span>
                      </Button>
                    ))}
                  </div>
                )}
              </ScrollArea>
            </section>
          </ResizablePanel>

          <ResizableHandle />

          <ResizablePanel
            defaultSize="78%"
            minSize={360}
            className="min-w-0 overflow-hidden"
          >
            <ResizablePanelGroup orientation="vertical">
              <ResizablePanel
                defaultSize="55%"
                minSize={220}
                className="min-h-0 overflow-hidden"
              >
                <section className="flex h-full min-h-0 flex-col">
                  <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
                    <p className="text-sm font-medium">
                      {selectedCue
                        ? m.workspace_cue({ index: selectedCue.cue_index })
                        : m.workspace_preview()}
                    </p>
                  </div>

                  <div className="@container min-h-0 flex-1 bg-muted/20 p-3">
                    {selectedCue ? (
                      <CueComparison
                        key={selectedCue.id}
                        projectPath={project.path}
                        cue={selectedCue}
                        document={previewDocument}
                        position={cueDraft?.position ?? selectedCue.position}
                      />
                    ) : (
                      <Empty className="h-full border-0">
                        <EmptyHeader>
                          <EmptyMedia variant="icon">
                            <SparklesIcon />
                          </EmptyMedia>
                          <EmptyTitle>{m.workspace_empty_title()}</EmptyTitle>
                          <EmptyDescription>
                            {m.workspace_empty_description()}
                          </EmptyDescription>
                        </EmptyHeader>
                      </Empty>
                    )}
                  </div>
                </section>
              </ResizablePanel>

              <ResizableHandle />

              <ResizablePanel
                defaultSize="45%"
                minSize={INSPECTOR_MIN_HEIGHT}
                maxSize={inspectorMaxHeight}
                groupResizeBehavior="preserve-pixel-size"
                panelRef={inspectorPanelRef}
                className="min-h-0 overflow-hidden"
              >
                <Inspector
                  cue={selectedCue}
                  document={selectedDocument}
                  draft={cueDraft}
                  changed={draftChanged}
                  saving={saving}
                  onDraftChange={updateCueDraft}
                  onSave={() => void saveCue()}
                  ocrBusy={ocrState !== "idle"}
                  translationBusy={translationBusy}
                  onOcr={() => {
                    if (selectedCue) void runOcr([selectedCue.id])
                  }}
                  onTranslate={() => {
                    if (selectedCue) void translate([selectedCue.id])
                  }}
                  onApprove={() => void reviewSelectedCue()}
                  onOpenHistory={() => setRevisionHistoryOpen(true)}
                  cuePosition={selectedIndex + 1}
                  cueCount={filteredCues.length}
                  onPrevious={() => selectAdjacentCue(-1)}
                  onNext={() => selectAdjacentCue(1)}
                  previousDisabled={selectedIndex <= 0}
                  nextDisabled={
                    selectedIndex < 0 ||
                    selectedIndex >= filteredCues.length - 1
                  }
                  onContentHeightChange={handleInspectorContentHeightChange}
                />
              </ResizablePanel>
            </ResizablePanelGroup>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>

      <footer className="flex h-6 shrink-0 items-center border-t px-3 text-xs text-muted-foreground">
        <span>{statusMessage}</span>
        {ocrProgress && ocrState !== "idle" && (
          <span className="ml-3 tabular-nums">
            {m.workspace_ocr_progress({
              current: ocrProgress.current,
              total: ocrProgress.total,
              state: ocrControlStateLabel(ocrState),
            })}
          </span>
        )}
        {translationProgress && translationBusy && (
          <span className="ml-3 tabular-nums">
            {m.workspace_translation_progress({
              current: translationProgress.current,
              total: translationProgress.total,
            })}
          </span>
        )}
      </footer>

      <SettingsDialog
        open={settingsOpen}
        settings={settings}
        debugLogCount={debugLogs.length}
        onOpenChange={setSettingsOpen}
        onOpenDebugLog={() => {
          setSettingsOpen(false)
          setDebugLogOpen(true)
        }}
        onSave={(next) => {
          setSettings(next)
          saveWorkspaceSettings(next)
          setError(null)
          setStatusMessage(m.status_settings_saved())
        }}
      />
      <DebugLogDialog
        open={debugLogOpen}
        entries={debugLogs}
        onOpenChange={(open) => {
          setDebugLogOpen(open)
          if (!open) setSettingsOpen(true)
        }}
        onClear={() => setDebugLogs([])}
      />
      <RevisionHistoryDialog
        open={revisionHistoryOpen}
        projectPath={project.path}
        cue={selectedCue}
        onOpenChange={setRevisionHistoryOpen}
        onChanged={async (message) => {
          if (selectedCue) {
            setDraftByCue((current) => {
              const next = { ...current }
              delete next[selectedCue.id]
              return next
            })
          }
          await loadDocument()
          setStatusMessage(message)
        }}
      />
      <SaveProjectAsDialog
        key={project.path}
        open={saveAsOpen}
        project={project}
        onOpenChange={setSaveAsOpen}
        onSaved={(next) => {
          onProjectChange(next)
          setStatusMessage(
            m.status_project_saved_as({ name: next.metadata.name })
          )
        }}
      />
      <SourceImportDialog
        open={sourceImportOpen}
        projectPath={project.path}
        onOpenChange={setSourceImportOpen}
        onImported={(result: SourceImportResult) => {
          onProjectChange(result.project)
          void loadDocument()
          setStatusMessage(
            m.status_source_imported({ name: result.source.display_name })
          )
        }}
      />
      {document && (
        <>
          <PgsExtractionDialog
            open={pgsOpen}
            document={document}
            progress={pgsProgress}
            busy={pgsBusy}
            error={pgsError}
            onOpenChange={setPgsOpen}
            onExtract={(sourceId, titleIndex, streamIndex) =>
              void extractPgs(sourceId, titleIndex, streamIndex)
            }
          />
          <ExportDialog
            open={exportOpen}
            document={document}
            onOpenChange={setExportOpen}
            onExport={exportSubtitles}
          />
        </>
      )}
    </main>
  )
}

function RibbonStatistic({ value, label }: { value: number; label: string }) {
  return (
    <div className="flex items-baseline gap-1.5 whitespace-nowrap">
      <span className="text-lg font-semibold text-foreground tabular-nums">
        {value.toLocaleString(getLocale())}
      </span>
      <span className="text-xs text-muted-foreground">{label}</span>
    </div>
  )
}

function CueComparison({
  projectPath,
  cue,
  document,
  position,
}: {
  projectPath: string
  cue: SubtitleCue
  document: OcrDocument | null
  position: SubtitlePosition
}) {
  const [imageUrl, setImageUrl] = React.useState<string | null>(null)
  const [imageError, setImageError] = React.useState<string | null>(null)
  const imagePlacement = cueImagePlacement(cue.geometry)
  const subtitlePlacement = cueSubtitlePlacement(
    cue.geometry,
    cue.position,
    position,
    document?.lines.length ?? 1
  )

  React.useEffect(() => {
    let active = true
    let objectUrl: string | null = null
    desktop
      .invoke<number[]>("cue_image", {
        projectPath,
        imagePath: cue.image_path,
      })
      .then((bytes) => {
        if (!active) {
          return
        }
        objectUrl = URL.createObjectURL(
          new Blob([Uint8Array.from(bytes)], { type: "image/png" })
        )
        setImageUrl(objectUrl)
      })
      .catch((reason: unknown) => {
        if (active) {
          setImageError(
            reason instanceof Error ? reason.message : String(reason)
          )
        }
      })

    return () => {
      active = false
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl)
      }
    }
  }, [cue.image_path, projectPath])

  return (
    <div className="grid h-full min-h-0 grid-cols-1 grid-rows-2 overflow-hidden rounded-xl border bg-border shadow-sm @[44rem]:grid-cols-2 @[44rem]:grid-rows-1">
      <div className="flex min-w-0 flex-col bg-card">
        <div className="flex h-9 shrink-0 items-center border-b px-3 text-xs font-medium">
          {m.workspace_preview_original()}
        </div>
        <div className="flex min-h-0 flex-1 items-center justify-center bg-preview p-5">
          {imageUrl ? (
            <svg
              className="h-full w-full"
              viewBox={`0 0 ${imagePlacement.canvasWidth} ${imagePlacement.canvasHeight}`}
              preserveAspectRatio="xMidYMid meet"
              role="img"
              aria-label={m.workspace_cue_image_aria({
                index: cue.cue_index,
              })}
            >
              <image
                href={imageUrl}
                x={imagePlacement.x}
                y={imagePlacement.y}
                width={imagePlacement.width}
                height={imagePlacement.height}
                preserveAspectRatio="none"
              />
            </svg>
          ) : imageError ? (
            <p className="text-xs text-preview-muted">
              {m.workspace_cue_image_unavailable()}
            </p>
          ) : (
            <Spinner className="text-preview-foreground" />
          )}
        </div>
      </div>
      <div className="flex min-w-0 flex-col bg-card">
        <div className="flex h-9 shrink-0 items-center border-b px-3 text-xs font-medium">
          {m.workspace_preview_subtitle()}
        </div>
        <div className="flex min-h-0 flex-1 items-center justify-center bg-preview p-5">
          <svg
            className="h-full w-full"
            viewBox={`0 0 ${subtitlePlacement.canvasWidth} ${subtitlePlacement.canvasHeight}`}
            preserveAspectRatio="xMidYMid meet"
            role="img"
            aria-label={m.workspace_preview_aria({ index: cue.cue_index })}
          >
            <foreignObject
              x={subtitlePlacement.x}
              y={subtitlePlacement.y}
              width={subtitlePlacement.width}
              height={subtitlePlacement.height}
              overflow="visible"
            >
              <div
                className="flex h-full w-full flex-col justify-center"
                style={{
                  alignItems: subtitlePlacement.alignItems,
                  textAlign: subtitlePlacement.textAlign,
                }}
              >
                <RenderedSubtitle
                  document={document}
                  fontSize={subtitlePlacement.fontSize}
                  lineHeight={subtitlePlacement.lineHeight}
                />
              </div>
            </foreignObject>
          </svg>
        </div>
      </div>
    </div>
  )
}

function PositionGrid({
  value,
  disabled,
  onValueChange,
}: {
  value: SubtitlePosition
  disabled: boolean
  onValueChange: (value: SubtitlePosition) => void
}) {
  return (
    <div className="flex flex-col gap-2">
      <div
        className="grid w-full max-w-44 grid-cols-3 gap-1.5"
        role="radiogroup"
        aria-label={m.workspace_subtitle_position()}
      >
        {subtitlePositions.map((position) => {
          const [vertical, horizontal] = position.split("-")
          const selected = position === value
          return (
            <Button
              key={position}
              type="button"
              variant={selected ? "secondary" : "outline"}
              className="h-9 rounded-lg p-1.5"
              role="radio"
              aria-checked={selected}
              aria-label={subtitlePositionLabel(position)}
              title={subtitlePositionLabel(position)}
              disabled={disabled}
              onClick={() => onValueChange(position)}
            >
              <span
                className={cn(
                  "flex size-full",
                  vertical === "top"
                    ? "items-start"
                    : vertical === "middle"
                      ? "items-center"
                      : "items-end",
                  horizontal === "left"
                    ? "justify-start"
                    : horizontal === "center"
                      ? "justify-center"
                      : "justify-end"
                )}
              >
                <span
                  className={cn(
                    "size-1.5 rounded-full",
                    selected ? "bg-foreground" : "bg-muted-foreground/60"
                  )}
                />
              </span>
            </Button>
          )
        })}
      </div>
      <p className="text-xs text-muted-foreground">
        {subtitlePositionLabel(value)}
      </p>
    </div>
  )
}

function Inspector({
  cue,
  document,
  draft,
  changed,
  saving,
  onDraftChange,
  onSave,
  ocrBusy,
  translationBusy,
  onOcr,
  onTranslate,
  onApprove,
  onOpenHistory,
  cuePosition,
  cueCount,
  onPrevious,
  onNext,
  previousDisabled,
  nextDisabled,
  onContentHeightChange,
}: {
  cue: SubtitleCue | null
  document: OcrDocument | null
  draft: CueEditDraft | null
  changed: boolean
  saving: boolean
  onDraftChange: (patch: Partial<CueEditDraft>) => void
  onSave: () => void
  ocrBusy: boolean
  translationBusy: boolean
  onOcr: () => void
  onTranslate: () => void
  onApprove: () => void
  onOpenHistory: () => void
  cuePosition: number
  cueCount: number
  onPrevious: () => void
  onNext: () => void
  previousDisabled: boolean
  nextDisabled: boolean
  onContentHeightChange: (height: number) => void
}) {
  const headerRef = React.useRef<HTMLDivElement>(null)
  const contentRef = React.useRef<HTMLDivElement>(null)

  React.useLayoutEffect(() => {
    const header = headerRef.current
    const content = contentRef.current
    if (!header || !content) return

    const reportHeight = () => {
      onContentHeightChange(
        Math.ceil(
          header.getBoundingClientRect().height +
            content.getBoundingClientRect().height
        )
      )
    }
    const observer = new ResizeObserver(reportHeight)
    observer.observe(header)
    observer.observe(content)
    reportHeight()

    return () => observer.disconnect()
  }, [onContentHeightChange])

  return (
    <aside className="flex h-full min-h-0 flex-col bg-background">
      <div
        ref={headerRef}
        className="flex min-h-11 shrink-0 flex-wrap items-center gap-2 border-b px-3 py-1.5"
      >
        <h2 className="text-sm font-medium">{m.workspace_inspector()}</h2>
        {cue && (
          <>
            <Badge variant="secondary">
              {m.workspace_ocr_status({
                status: ocrStatusLabel(cue.ocr_status),
              })}
            </Badge>
            <Badge variant="outline">
              {reviewStatusLabel(cue.review_status)}
            </Badge>
            <div className="ml-auto flex flex-wrap items-center gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={ocrBusy}
                onClick={onOcr}
              >
                {ocrBusy ? (
                  <Spinner data-icon="inline-start" />
                ) : (
                  <SparklesIcon data-icon="inline-start" />
                )}
                {m.workspace_ocr_cue()}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={!document || translationBusy}
                onClick={onTranslate}
              >
                {translationBusy ? (
                  <Spinner data-icon="inline-start" />
                ) : (
                  <LanguagesIcon data-icon="inline-start" />
                )}
                {m.workspace_translate()}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={!document || cue.review_status === "approved"}
                onClick={onApprove}
              >
                <CheckCircle2Icon data-icon="inline-start" />
                {cue.review_status === "approved"
                  ? m.workspace_reviewed()
                  : m.workspace_mark_reviewed()}
              </Button>
              <Separator orientation="vertical" className="mx-1" />
              <span className="text-xs whitespace-nowrap text-muted-foreground tabular-nums">
                {m.workspace_cue({ index: cue.cue_index })} ·{" "}
                {m.workspace_cue_position({
                  current: cuePosition,
                  total: cueCount,
                })}
              </span>
              <IconButton
                label={m.workspace_previous_cue()}
                icon={ChevronLeftIcon}
                onClick={onPrevious}
                disabled={previousDisabled}
              />
              <IconButton
                label={m.workspace_next_cue()}
                icon={ChevronRightIcon}
                onClick={onNext}
                disabled={nextDisabled}
              />
            </div>
          </>
        )}
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div ref={contentRef}>
          {cue ? (
            <div className="grid gap-4 p-3 lg:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
              <section className="min-w-0">
                <FieldGroup>
                  <Field>
                    <div className="flex items-center gap-2">
                      <FieldLabel htmlFor="cue-content">
                        {m.workspace_content()}
                      </FieldLabel>
                      <Badge
                        variant={changed ? "secondary" : "ghost"}
                        className="ml-auto"
                      >
                        {changed ? m.workspace_unsaved() : m.workspace_saved()}
                      </Badge>
                      <IconButton
                        label={m.revision_history_title()}
                        icon={HistoryIcon}
                        size="icon-xs"
                        onClick={onOpenHistory}
                      />
                    </div>
                    <SubtitleContentEditor
                      lines={draft?.lines ?? []}
                      disabled={!document || saving}
                      onChange={(lines) => onDraftChange({ lines })}
                    />
                    <Button onClick={onSave} disabled={!changed || saving}>
                      {saving ? (
                        <Spinner data-icon="inline-start" />
                      ) : (
                        <SaveIcon data-icon="inline-start" />
                      )}
                      {m.workspace_save_cue()}
                    </Button>
                  </Field>
                </FieldGroup>
              </section>

              <section className="flex min-w-0 flex-col gap-3 border-t pt-3 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-4">
                <div className="flex items-center gap-2">
                  <p className="text-sm font-medium">
                    {m.workspace_timing_placement()}
                  </p>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  <Field>
                    <FieldLabel htmlFor="cue-start">
                      {m.workspace_start()}
                    </FieldLabel>
                    <Input
                      id="cue-start"
                      value={draft?.start ?? formatTimestamp(cue.start_ms)}
                      disabled={!document || saving}
                      onChange={(event) =>
                        onDraftChange({ start: event.target.value })
                      }
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor="cue-end">
                      {m.workspace_end()}
                    </FieldLabel>
                    <Input
                      id="cue-end"
                      value={draft?.end ?? formatTimestamp(cue.end_ms)}
                      disabled={!document || saving}
                      onChange={(event) =>
                        onDraftChange({ end: event.target.value })
                      }
                    />
                  </Field>
                </div>
                <Field>
                  <FieldLabel>{m.workspace_position()}</FieldLabel>
                  <PositionGrid
                    value={draft?.position ?? cue.position}
                    disabled={!document || saving}
                    onValueChange={(position) => onDraftChange({ position })}
                  />
                </Field>
              </section>
            </div>
          ) : (
            <Empty className="border-0 p-6">
              <EmptyHeader>
                <EmptyTitle>{m.workspace_no_cue_selected()}</EmptyTitle>
                <EmptyDescription>
                  {m.workspace_no_cue_description()}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </div>
      </ScrollArea>
    </aside>
  )
}
