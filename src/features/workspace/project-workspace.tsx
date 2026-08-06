import * as React from "react"
import type { LucideIcon } from "lucide-react"
import {
  CheckCircle2Icon,
  ChevronLeftIcon,
  ChevronRightIcon,
  DownloadIcon,
  FileOutputIcon,
  FolderInputIcon,
  LanguagesIcon,
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

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { SettingsDialog } from "@/features/settings/settings-dialog"
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
import { normalizeOcrLines } from "@/features/workspace/subtitle-spans"
import { desktop } from "@/lib/desktop"
import { cn } from "@/lib/utils"

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
    | "approve"
    | "refresh"
    | "translate-cue"
    | "translate-all"
  label: string
  icon: LucideIcon
}

const ribbonTabs: Array<{
  value: string
  label: string
  commands: RibbonCommand[]
}> = [
  {
    value: "project",
    label: "Project",
    commands: [
      { id: "save", label: "Save", icon: SaveIcon },
      { id: "save-as", label: "Save As", icon: FileOutputIcon },
      { id: "import-source", label: "Import Source", icon: FolderInputIcon },
      { id: "export", label: "Export", icon: DownloadIcon },
    ],
  },
  {
    value: "edit",
    label: "Edit",
    commands: [
      { id: "undo", label: "Undo", icon: Undo2Icon },
      { id: "redo", label: "Redo", icon: Redo2Icon },
    ],
  },
  {
    value: "subtitle",
    label: "Subtitle",
    commands: [
      { id: "extract-pgs", label: "Extract PGS", icon: SparklesIcon },
      { id: "start-ocr", label: "Start Full OCR", icon: PlayIcon },
      { id: "pause-ocr", label: "Pause", icon: PauseIcon },
      { id: "resume-ocr", label: "Resume", icon: PlayIcon },
      { id: "stop-ocr", label: "Stop", icon: SquareIcon },
    ],
  },
  {
    value: "review",
    label: "Review",
    commands: [
      { id: "approve", label: "Mark Reviewed", icon: CheckCircle2Icon },
      { id: "refresh", label: "Refresh Cues", icon: RefreshCwIcon },
    ],
  },
  {
    value: "translate",
    label: "Translate",
    commands: [
      { id: "translate-cue", label: "Translate Cue", icon: LanguagesIcon },
      { id: "translate-all", label: "Translate All", icon: PlayIcon },
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
  return value
    .split("-")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ")
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

function RenderedSubtitle({
  document,
  fontSize,
  lineHeight,
}: {
  document: OcrDocument | null
  fontSize: number
  lineHeight: number
}) {
  if (!document) {
    return (
      <p className="text-preview-muted" style={{ fontSize }}>
        No OCR text yet
      </p>
    )
  }

  return (
    <div className="flex w-full flex-col text-preview-foreground [text-shadow:0_1px_2px_var(--preview-shadow),0_0_1px_var(--preview-shadow)]">
      {document.lines.map((line, lineIndex) => (
        <p
          key={`${line.text}-${lineIndex}`}
          className="m-0 font-medium whitespace-pre"
          style={{ fontSize, lineHeight: `${lineHeight}px` }}
        >
          {line.spans.length === 0 ? (
            <span>{line.text}</span>
          ) : (
            line.spans.map((span, spanIndex) =>
              span.type === "text" ? (
                <span
                  key={spanIndex}
                  className={cn(
                    span.styles.includes("bold") && "font-bold",
                    span.styles.includes("italic") && "italic",
                    span.styles.includes("underline") && "underline",
                    span.styles.includes("strikethrough") && "line-through",
                    span.styles.includes("superscript") &&
                      "align-super text-[0.75em] leading-none",
                    span.styles.includes("subscript") &&
                      "align-sub text-[0.75em] leading-none"
                  )}
                >
                  {span.text}
                </span>
              ) : (
                <ruby
                  key={spanIndex}
                  style={{
                    rubyPosition:
                      span.annotations[0]?.position === "under"
                        ? "under"
                        : "over",
                  }}
                  className={cn(
                    span.styles.includes("bold") && "font-bold",
                    span.styles.includes("italic") && "italic",
                    span.styles.includes("underline") && "underline",
                    span.styles.includes("strikethrough") && "line-through",
                    span.styles.includes("superscript") &&
                      "align-super text-[0.75em] leading-none",
                    span.styles.includes("subscript") &&
                      "align-sub text-[0.75em] leading-none"
                  )}
                >
                  {span.base}
                  {span.annotations.map((annotation, annotationIndex) => (
                    <rt key={annotationIndex}>{annotation.text}</rt>
                  ))}
                </ruby>
              )
            )
          )}
        </p>
      ))}
    </div>
  )
}

function IconButton({
  label,
  icon: Icon,
  onClick,
  disabled,
}: {
  label: string
  icon: LucideIcon
  onClick: () => void
  disabled?: boolean
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
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
  const [statusMessage, setStatusMessage] = React.useState("Ready")
  const [settings, setSettings] = React.useState<WorkspaceSettings>(
    loadWorkspaceSettings
  )
  const [settingsOpen, setSettingsOpen] = React.useState(false)
  const [saveAsOpen, setSaveAsOpen] = React.useState(false)
  const [sourceImportOpen, setSourceImportOpen] = React.useState(false)
  const [pgsOpen, setPgsOpen] = React.useState(false)
  const [exportOpen, setExportOpen] = React.useState(false)
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

  const loadDocument = React.useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const next = await desktop.invoke<ProjectDocument>("project_document", {
        path: project.path,
      })
      setDocument(next)
      setActiveCueId((current) =>
        current && next.cues.some((cue) => cue.id === current)
          ? current
          : (next.cues[0]?.id ?? null)
      )
      setStatusMessage("Project loaded")
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage("Unable to load project")
    } finally {
      setLoading(false)
    }
  }, [project.path])

  React.useEffect(() => {
    let active = true
    desktop
      .invoke<ProjectDocument>("project_document", { path: project.path })
      .then((next) => {
        if (!active) {
          return
        }
        setDocument(next)
        setActiveCueId(next.cues[0]?.id ?? null)
        setStatusMessage("Project loaded")
      })
      .catch((reason: unknown) => {
        if (!active) {
          return
        }
        setError(reason instanceof Error ? reason.message : String(reason))
        setStatusMessage("Unable to load project")
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
    return () => {
      removePgsListener()
      removeOcrListener()
      removeControlListener()
      removeTranslationListener()
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
      setError("Use HH:MM:SS.mmm timestamps and make End later than Start.")
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
      setStatusMessage(`Cue ${selectedCue.cue_index} saved`)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage("Unable to save cue")
    } finally {
      setSaving(false)
    }
  }

  const requireModel = (task: "ocr" | "validation" | "translation") => {
    const message = validateProviderConfig(settings.profiles[task])
    if (message) {
      setError(message)
      setSettingsOpen(true)
      setStatusMessage("Configure the task model to continue")
      return false
    }
    return true
  }

  const runOcr = async (cueIds?: string[]) => {
    if (!requireModel("ocr") || !requireModel("validation")) return
    setError(null)
    setOcrState("running")
    setOcrProgress(null)
    setStatusMessage(cueIds ? "Running OCR for selected Cue" : "OCR started")
    try {
      const result = await desktop.invoke<OcrJobResult>("recognize_ocr", {
        projectPath: project.path,
        cueIds: cueIds ?? null,
        language: settings.ocr_language,
        overwrite: Boolean(cueIds),
        config: {
          recognition: settings.profiles.ocr,
          validation: settings.profiles.validation,
        },
      })
      onProjectChange(result.project)
      await loadDocument()
      setStatusMessage(`OCR completed · ${result.processed} Cues processed`)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage("OCR stopped with an error")
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
          ? "OCR will pause at the next safe Cue boundary"
          : action === "resume"
            ? "OCR resumed"
            : "OCR will stop at the next safe Cue boundary"
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
      cueIds ? "Translating selected Cue" : "Translation started"
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
        `Translation completed · ${result.processed} Cues processed`
      )
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setStatusMessage("Translation stopped with an error")
    } finally {
      setTranslationBusy(false)
    }
  }

  const reviewSelectedCue = async () => {
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
      setStatusMessage(`Cue ${selectedCue.cue_index} marked reviewed`)
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
        setStatusMessage("No earlier Cue revision is available")
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
      setStatusMessage(`Undid Cue ${selectedCue.cue_index}`)
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
      setStatusMessage(`Redid Cue ${selectedCue.cue_index}`)
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
      setStatusMessage(`PGS extraction complete · ${result.cue_count} Cues`)
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
        return !selectedCue || document?.revisions.length === 0
      case "redo":
        return !selectedCue || !(redoByCue[selectedCue.id]?.length > 0)
      case "start-ocr":
        return ocrState !== "idle" || !document?.cues.length
      case "pause-ocr":
        return ocrState !== "running"
      case "resume-ocr":
        return ocrState !== "paused"
      case "stop-ocr":
        return ocrState === "idle" || ocrState === "stopping"
      case "approve":
      case "translate-cue":
        return !selectedDocument
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
        else setStatusMessage("Project is up to date")
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

      <Tabs defaultValue="project" className="shrink-0 gap-0">
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
            <div className="flex h-20 shrink-0 items-center gap-1 border-b bg-muted/30 px-3">
              {tab.commands.map((command) => {
                const Icon = command.icon
                return (
                  <Button
                    key={command.label}
                    variant="ghost"
                    className="h-14 flex-col gap-1 px-3"
                    disabled={commandDisabled(command)}
                    onClick={() => handleRibbonCommand(command)}
                  >
                    <Icon data-icon="inline-start" />
                    <span className="text-xs">{command.label}</span>
                  </Button>
                )
              })}
              <Separator orientation="vertical" className="mx-2" />
              <div className="ml-auto flex items-center gap-6 px-2">
                <RibbonStatistic value={statistics.cue_count} label="cues" />
                <RibbonStatistic
                  value={statistics.source_count}
                  label="sources"
                />
                <RibbonStatistic
                  value={statistics.ocr_completed_count}
                  label="OCR complete"
                />
                <RibbonStatistic
                  value={statistics.reviewed_count}
                  label="reviewed"
                />
              </div>
              <Separator orientation="vertical" className="mx-2" />
              <Button
                variant="ghost"
                className="h-14 flex-col gap-1 px-3"
                onClick={() => setSettingsOpen(true)}
              >
                <Settings2Icon data-icon="inline-start" />
                <span className="text-xs">Settings</span>
              </Button>
            </div>
          </TabsContent>
        ))}
      </Tabs>

      {error && (
        <Alert variant="destructive" className="m-3 shrink-0">
          <AlertTitle>Project action failed</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <div className="min-h-0 flex-1">
        <ResizablePanelGroup orientation="horizontal">
          <ResizablePanel defaultSize="22%" minSize="16%" maxSize="34%">
            <section className="flex h-full min-h-0 flex-col bg-muted/20">
              <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
                <h2 className="text-sm font-medium">Cue List</h2>
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
                    placeholder="Number or text"
                    aria-label="Search cues"
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
                      <EmptyTitle>No cues</EmptyTitle>
                      <EmptyDescription>
                        Import a source and extract a PGS track to begin.
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
                            {cueText.get(cue.id) || "Waiting for OCR"}
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

          <ResizablePanel defaultSize="78%" minSize="60%">
            <ResizablePanelGroup orientation="vertical">
              <ResizablePanel defaultSize="50%" minSize="30%">
                <section className="flex h-full min-h-0 flex-col">
                  <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
                    <p className="text-sm font-medium">
                      {selectedCue ? `Cue ${selectedCue.cue_index}` : "Preview"}
                    </p>
                    <div className="ml-auto flex items-center gap-1">
                      <IconButton
                        label="Previous cue"
                        icon={ChevronLeftIcon}
                        onClick={() => selectAdjacentCue(-1)}
                        disabled={selectedIndex <= 0}
                      />
                      <IconButton
                        label="Next cue"
                        icon={ChevronRightIcon}
                        onClick={() => selectAdjacentCue(1)}
                        disabled={
                          selectedIndex < 0 ||
                          selectedIndex >= filteredCues.length - 1
                        }
                      />
                    </div>
                  </div>

                  <div className="min-h-0 flex-1 bg-muted/20 p-3">
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
                          <EmptyTitle>Workspace ready</EmptyTitle>
                          <EmptyDescription>
                            Import a Blu-ray source, extract PGS cues, and begin
                            reviewing as OCR results arrive.
                          </EmptyDescription>
                        </EmptyHeader>
                      </Empty>
                    )}
                  </div>
                </section>
              </ResizablePanel>

              <ResizableHandle />

              <ResizablePanel defaultSize="50%" minSize="32%" maxSize="70%">
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
            OCR {ocrProgress.current}/{ocrProgress.total} · {ocrState}
          </span>
        )}
        {translationProgress && translationBusy && (
          <span className="ml-3 tabular-nums">
            Translation {translationProgress.current}/
            {translationProgress.total}
          </span>
        )}
      </footer>

      <SettingsDialog
        open={settingsOpen}
        settings={settings}
        onOpenChange={setSettingsOpen}
        onSave={(next) => {
          setSettings(next)
          saveWorkspaceSettings(next)
          setError(null)
          setStatusMessage("Settings saved")
        }}
      />
      <SaveProjectAsDialog
        key={project.path}
        open={saveAsOpen}
        project={project}
        onOpenChange={setSaveAsOpen}
        onSaved={(next) => {
          onProjectChange(next)
          setStatusMessage(`Project saved as ${next.metadata.name}`)
        }}
      />
      <SourceImportDialog
        open={sourceImportOpen}
        projectPath={project.path}
        onOpenChange={setSourceImportOpen}
        onImported={(result: SourceImportResult) => {
          onProjectChange(result.project)
          void loadDocument()
          setStatusMessage(`Source ${result.source.display_name} imported`)
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
        {value.toLocaleString()}
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
    <div className="grid h-full min-h-0 grid-cols-2 overflow-hidden rounded-xl border bg-border shadow-sm">
      <div className="flex min-w-0 flex-col bg-card">
        <div className="flex h-9 shrink-0 items-center border-b px-3 text-xs font-medium">
          Original cue
        </div>
        <div className="flex min-h-0 flex-1 items-center justify-center bg-preview p-5">
          {imageUrl ? (
            <svg
              className="h-full w-full"
              viewBox={`0 0 ${imagePlacement.canvasWidth} ${imagePlacement.canvasHeight}`}
              preserveAspectRatio="xMidYMid meet"
              role="img"
              aria-label={`Original subtitle cue ${cue.cue_index} at its Blu-ray canvas position`}
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
            <p className="text-xs text-preview-muted">Cue image unavailable</p>
          ) : (
            <Spinner className="text-preview-foreground" />
          )}
        </div>
      </div>
      <div className="flex min-w-0 flex-col bg-card">
        <div className="flex h-9 shrink-0 items-center border-b px-3 text-xs font-medium">
          Subtitle preview
        </div>
        <div className="flex min-h-0 flex-1 items-center justify-center bg-preview p-5">
          <svg
            className="h-full w-full"
            viewBox={`0 0 ${subtitlePlacement.canvasWidth} ${subtitlePlacement.canvasHeight}`}
            preserveAspectRatio="xMidYMid meet"
            role="img"
            aria-label={`Generated subtitle preview for cue ${cue.cue_index}`}
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
        aria-label="Subtitle position"
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
}) {
  return (
    <aside className="flex h-full min-h-0 flex-col bg-background">
      <div className="flex min-h-11 shrink-0 items-center gap-2 border-b px-3 py-1.5">
        <h2 className="text-sm font-medium">Inspector</h2>
        {cue && (
          <>
            <span className="text-xs text-muted-foreground">
              Cue {cue.cue_index}
            </span>
            <Badge variant="outline">{cue.review_status}</Badge>
            <div className="ml-auto flex items-center gap-2">
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
                OCR Cue
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
                Translate
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={!document || cue.review_status === "approved"}
                onClick={onApprove}
              >
                <CheckCircle2Icon data-icon="inline-start" />
                {cue.review_status === "approved"
                  ? "Reviewed"
                  : "Mark as Reviewed"}
              </Button>
            </div>
          </>
        )}
      </div>
      {cue ? (
        <ScrollArea className="min-h-0 flex-1">
          <div className="grid gap-4 p-3 lg:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
            <section className="min-w-0">
              <FieldGroup>
                <Field>
                  <div className="flex items-center gap-2">
                    <FieldLabel htmlFor="cue-content">Content</FieldLabel>
                    <Badge
                      variant={changed ? "secondary" : "ghost"}
                      className="ml-auto"
                    >
                      {changed ? "Unsaved" : "Saved"}
                    </Badge>
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
                    Save Cue
                  </Button>
                </Field>
              </FieldGroup>
            </section>

            <section className="flex min-w-0 flex-col gap-3 border-t pt-3 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-4">
              <div className="flex items-center gap-2">
                <p className="text-sm font-medium">Timing & placement</p>
                <Badge variant="secondary" className="ml-auto">
                  {cue.ocr_status}
                </Badge>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <Field>
                  <FieldLabel htmlFor="cue-start">Start</FieldLabel>
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
                  <FieldLabel htmlFor="cue-end">End</FieldLabel>
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
                <FieldLabel>Position</FieldLabel>
                <PositionGrid
                  value={draft?.position ?? cue.position}
                  disabled={!document || saving}
                  onValueChange={(position) => onDraftChange({ position })}
                />
              </Field>
            </section>
          </div>
        </ScrollArea>
      ) : (
        <Empty className="border-0 p-6">
          <EmptyHeader>
            <EmptyTitle>No Cue Selected</EmptyTitle>
            <EmptyDescription>
              Choose a cue to edit content, timing, position, and style.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </aside>
  )
}
