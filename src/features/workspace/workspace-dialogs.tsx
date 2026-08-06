import * as React from "react"
import {
  CheckCircle2Icon,
  Disc3Icon,
  DownloadIcon,
  FolderOpenIcon,
  SaveIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Progress,
  ProgressLabel,
  ProgressValue,
} from "@/components/ui/progress"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import type {
  BlurayDiscInfo,
  ExportFormat,
  ExportOptions,
  ExportResult,
  ExportScope,
  PgsExtractionProgress,
  ProjectDocument,
  ProjectOverview,
  SourceImportResult,
} from "@/features/projects/types"
import { desktop } from "@/lib/desktop"
import { projectNameError } from "@/lib/project-name"
import * as m from "@/paraglide/messages.js"

function parentDirectory(path: string) {
  const normalized = path.replaceAll("\\", "/")
  const separator = normalized.lastIndexOf("/")
  return separator > 0 ? normalized.slice(0, separator) : normalized
}

function formatDuration(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const remaining = seconds % 60
  return [hours, minutes, remaining]
    .map((part) => String(part).padStart(2, "0"))
    .join(":")
}

function progressPhaseLabel(value: string) {
  const labels: Record<string, () => string> = {
    queued: m.progress_queued,
    demuxing: m.progress_demuxing,
    decoding: m.progress_decoding,
    running: m.progress_running,
    "cue-complete": m.progress_cue_complete,
    completed: m.progress_completed,
    stopped: m.progress_stopped,
    failed: m.progress_failed,
  }
  return labels[value]?.() ?? value
}

export function SaveProjectAsDialog({
  open,
  project,
  onOpenChange,
  onSaved,
}: {
  open: boolean
  project: ProjectOverview
  onOpenChange: (open: boolean) => void
  onSaved: (project: ProjectOverview) => void
}) {
  const [name, setName] = React.useState<string>(() =>
    m.save_as_copy_name({ name: project.metadata.name })
  )
  const [parent, setParent] = React.useState(parentDirectory(project.path))
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setName(m.save_as_copy_name({ name: project.metadata.name }))
      setParent(parentDirectory(project.path))
      setError(null)
    }
    onOpenChange(nextOpen)
  }

  const invalidName = projectNameError(name)

  const chooseParent = async () => {
    const selected = await desktop.dialogs.selectDirectory({
      title: m.save_as_choose_location(),
      defaultPath: parent,
    })
    if (selected) setParent(selected)
  }

  const save = async (event: React.FormEvent) => {
    event.preventDefault()
    if (invalidName || !parent) return
    setBusy(true)
    setError(null)
    try {
      const result = await desktop.invoke<ProjectOverview>("save_project_as", {
        projectPath: project.path,
        parent,
        name: name.trim(),
      })
      onSaved(result)
      onOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <form className="flex flex-col gap-6" onSubmit={save}>
          <DialogHeader>
            <DialogTitle>{m.save_as_title()}</DialogTitle>
            <DialogDescription>{m.save_as_description()}</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field data-invalid={Boolean(invalidName) || undefined}>
              <FieldLabel htmlFor="save-as-name">
                {m.field_project_name()}
              </FieldLabel>
              <Input
                id="save-as-name"
                value={name}
                autoFocus
                onChange={(event) => setName(event.target.value)}
              />
              {invalidName && <FieldError>{invalidName}</FieldError>}
            </Field>
            <Field>
              <FieldLabel>{m.save_as_destination()}</FieldLabel>
              <Button
                type="button"
                variant="outline"
                onClick={() => void chooseParent()}
              >
                <FolderOpenIcon />
                {m.common_choose_folder()}
              </Button>
              <FieldDescription>{parent}</FieldDescription>
            </Field>
            {error && <FieldError>{error}</FieldError>}
          </FieldGroup>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => handleOpenChange(false)}
            >
              {m.common_cancel()}
            </Button>
            <Button type="submit" disabled={busy || Boolean(invalidName)}>
              {busy ? <Spinner /> : <SaveIcon />}
              {m.save_as_save_copy()}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function SourceImportDialog({
  open,
  projectPath,
  onOpenChange,
  onImported,
}: {
  open: boolean
  projectPath: string
  onOpenChange: (open: boolean) => void
  onImported: (result: SourceImportResult) => void
}) {
  const [disc, setDisc] = React.useState<BlurayDiscInfo | null>(null)
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setDisc(null)
      setError(null)
    }
    onOpenChange(nextOpen)
  }

  const mainTitle = disc?.titles.find(
    (title) => title.index === disc.main_title_index
  )

  const chooseSource = async () => {
    const selected = await desktop.dialogs.selectDirectory({
      title: m.source_choose_folder(),
    })
    if (!selected) return
    setBusy(true)
    setError(null)
    try {
      setDisc(
        await desktop.invoke<BlurayDiscInfo>("inspect_bluray_source", {
          path: selected,
        })
      )
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  const attach = async () => {
    if (!disc) return
    setBusy(true)
    setError(null)
    try {
      const result = await desktop.invoke<SourceImportResult>(
        "attach_bluray_source",
        {
          projectPath,
          sourcePath: disc.root_path,
        }
      )
      onImported(result)
      handleOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{m.source_import()}</DialogTitle>
          <DialogDescription>{m.source_description()}</DialogDescription>
        </DialogHeader>

        {!disc ? (
          <Button
            variant="outline"
            className="h-36 flex-col gap-3 border-dashed"
            disabled={busy}
            onClick={() => void chooseSource()}
          >
            {busy ? (
              <Spinner className="size-6" />
            ) : (
              <Disc3Icon className="size-7" />
            )}
            <span>
              {busy ? m.source_analyzing() : m.source_choose_bluray()}
            </span>
          </Button>
        ) : (
          <div className="flex flex-col gap-4 rounded-xl border p-4">
            <div className="flex items-start gap-3">
              <CheckCircle2Icon className="mt-0.5 size-5 text-emerald-600" />
              <div className="min-w-0 flex-1">
                <p className="font-medium">{disc.display_name}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {disc.root_path}
                </p>
              </div>
              <Badge variant="secondary">
                {m.source_title_count({ count: disc.titles.length })}
              </Badge>
            </div>
            {mainTitle && (
              <div className="grid grid-cols-4 gap-3 text-sm">
                <Metric
                  label={m.source_main_title()}
                  value={`#${mainTitle.index}`}
                />
                <Metric
                  label={m.source_playlist()}
                  value={mainTitle.playlist}
                />
                <Metric
                  label={m.source_duration()}
                  value={formatDuration(mainTitle.duration_seconds)}
                />
                <Metric
                  label={m.source_pgs_tracks()}
                  value={mainTitle.pgs_tracks}
                />
              </div>
            )}
          </div>
        )}

        {error && (
          <Alert variant="destructive">
            <AlertTitle>{m.source_analysis_failed()}</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        <DialogFooter>
          {disc && (
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => void chooseSource()}
            >
              {m.source_choose_another()}
            </Button>
          )}
          <Button
            type="button"
            variant="outline"
            disabled={busy}
            onClick={() => handleOpenChange(false)}
          >
            {m.common_cancel()}
          </Button>
          {disc && (
            <Button disabled={busy} onClick={() => void attach()}>
              {busy ? <Spinner /> : <Disc3Icon />}
              {m.source_add()}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function PgsExtractionDialog({
  open,
  document,
  progress,
  busy,
  error,
  onOpenChange,
  onExtract,
}: {
  open: boolean
  document: ProjectDocument
  progress: PgsExtractionProgress | null
  busy: boolean
  error: string | null
  onOpenChange: (open: boolean) => void
  onExtract: (sourceId: string, titleIndex: number, streamIndex: number) => void
}) {
  const [sourceId, setSourceId] = React.useState("")
  const source =
    document.sources.find((candidate) => candidate.id === sourceId) ??
    document.sources[0]
  const disc = source?.metadata.data
  const [titleIndex, setTitleIndex] = React.useState<number | null>(null)
  const resolvedTitleIndex = titleIndex ?? disc?.main_title_index
  const title =
    disc?.titles.find((candidate) => candidate.index === resolvedTitleIndex) ??
    disc?.titles[0]
  const available = Array.from(
    { length: title?.pgs_tracks ?? 0 },
    (_, index) => ({
      index,
      language: title?.pgs_languages[index] ?? "und",
      extracted: document.tracks.some(
        (track) =>
          track.source_id === source?.id &&
          track.stream_index === index &&
          track.metadata.data.title_index === title?.index
      ),
    })
  )
  const firstAvailable = available.find((track) => !track.extracted)?.index ?? 0
  const [streamIndex, setStreamIndex] = React.useState<number | null>(null)
  const resolvedStreamIndex = streamIndex ?? firstAvailable

  const selectedTrack = available.find(
    (track) => track.index === resolvedStreamIndex
  )
  const sourceItems = document.sources.map((candidate) => ({
    value: candidate.id,
    label: candidate.display_name,
  }))
  const titleItems = (disc?.titles ?? []).map((candidate) => ({
    value: String(candidate.index),
    label: `#${candidate.index} · ${candidate.playlist} · ${formatDuration(candidate.duration_seconds)} · ${candidate.pgs_tracks} PGS`,
  }))
  const streamItems = available.map((track) => ({
    value: String(track.index),
    label: `PGS ${track.index + 1} · ${track.language.toUpperCase()}${track.extracted ? ` · ${m.pgs_already_extracted()}` : ""}`,
  }))
  const progressValue = progress?.estimated_total
    ? Math.min(100, (progress.current / progress.estimated_total) * 100)
    : null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{m.pgs_extract_track()}</DialogTitle>
          <DialogDescription>{m.pgs_description()}</DialogDescription>
        </DialogHeader>

        {source && disc && title ? (
          <FieldGroup>
            {document.sources.length > 1 && (
              <Field>
                <FieldLabel>{m.pgs_source()}</FieldLabel>
                <Select
                  items={sourceItems}
                  value={source.id}
                  onValueChange={(value) => {
                    setSourceId(String(value))
                    setTitleIndex(null)
                    setStreamIndex(null)
                  }}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {document.sources.map((candidate) => (
                        <SelectItem key={candidate.id} value={candidate.id}>
                          {candidate.display_name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
            )}
            <Field>
              <FieldLabel>{m.pgs_title()}</FieldLabel>
              <Select
                items={titleItems}
                value={String(title.index)}
                onValueChange={(value) => {
                  setTitleIndex(Number(value))
                  setStreamIndex(null)
                }}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {disc.titles.map((candidate) => (
                      <SelectItem
                        key={candidate.index}
                        value={String(candidate.index)}
                        disabled={candidate.pgs_tracks === 0}
                      >
                        #{candidate.index} · {candidate.playlist} ·{" "}
                        {formatDuration(candidate.duration_seconds)} ·{" "}
                        {candidate.pgs_tracks} PGS
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel>{m.pgs_stream()}</FieldLabel>
              <Select
                items={streamItems}
                value={String(resolvedStreamIndex)}
                onValueChange={(value) => setStreamIndex(Number(value))}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {available.map((track) => (
                      <SelectItem
                        key={track.index}
                        value={String(track.index)}
                        disabled={track.extracted}
                      >
                        PGS {track.index + 1} · {track.language.toUpperCase()}
                        {track.extracted
                          ? ` · ${m.pgs_already_extracted()}`
                          : ""}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
          </FieldGroup>
        ) : (
          <Alert variant="destructive">
            <AlertTitle>{m.pgs_no_source()}</AlertTitle>
            <AlertDescription>{m.pgs_import_first()}</AlertDescription>
          </Alert>
        )}

        {progress && (
          <Progress value={progressValue}>
            <ProgressLabel className="capitalize">
              {progressPhaseLabel(progress.phase)}
            </ProgressLabel>
            <ProgressValue />
          </Progress>
        )}
        {error && <FieldError>{error}</FieldError>}

        <DialogFooter>
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {m.common_cancel()}
          </Button>
          <Button
            disabled={
              !source ||
              !title ||
              !selectedTrack ||
              selectedTrack.extracted ||
              busy
            }
            onClick={() => {
              if (source && title)
                onExtract(source.id, title.index, resolvedStreamIndex)
            }}
          >
            {busy ? <Spinner /> : <Disc3Icon />}
            {m.pgs_start_extraction()}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function ExportDialog({
  open,
  document,
  onOpenChange,
  onExport,
}: {
  open: boolean
  document: ProjectDocument
  onOpenChange: (open: boolean) => void
  onExport: (options: ExportOptions) => Promise<ExportResult>
}) {
  const [trackId, setTrackId] = React.useState(document.tracks[0]?.id ?? "")
  const [formats, setFormats] = React.useState<ExportFormat[]>(["json", "srt"])
  const [scope, setScope] = React.useState<ExportScope>("all_recognized")
  const [outputDirectory, setOutputDirectory] = React.useState(
    `${document.project.path}/exports`
  )
  const [baseName, setBaseName] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [result, setResult] = React.useState<ExportResult | null>(null)
  const resolvedTrackId = trackId || document.tracks[0]?.id || ""
  const trackItems = document.tracks.map((track, index) => ({
    value: track.id,
    label: `${track.language?.toUpperCase() ?? "UND"} · PGS ${index + 1}`,
  }))
  const scopeItems: Array<{ value: ExportScope; label: string }> = [
    { value: "all_recognized", label: m.export_scope_all() },
    { value: "approved_only", label: m.export_scope_approved() },
  ]

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setTrackId(document.tracks[0]?.id ?? "")
      setResult(null)
      setError(null)
    }
    onOpenChange(nextOpen)
  }

  const toggleFormat = (format: ExportFormat) => {
    setFormats((current) =>
      current.includes(format)
        ? current.filter((candidate) => candidate !== format)
        : [...current, format]
    )
  }

  const chooseOutput = async () => {
    const selected = await desktop.dialogs.selectDirectory({
      title: m.export_choose_folder(),
      defaultPath: outputDirectory,
    })
    if (selected) setOutputDirectory(selected)
  }

  const startExport = async () => {
    setBusy(true)
    setError(null)
    try {
      setResult(
        await onExport({
          track_id: resolvedTrackId || null,
          formats,
          scope,
          output_directory: outputDirectory,
          base_name: baseName.trim() || null,
        })
      )
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{m.export_subtitles()}</DialogTitle>
          <DialogDescription>
            {m.export_canonical_description()}
          </DialogDescription>
        </DialogHeader>

        {!result ? (
          <FieldGroup>
            <Field>
              <FieldLabel>{m.export_track()}</FieldLabel>
              <Select
                items={trackItems}
                value={resolvedTrackId}
                onValueChange={(value) => setTrackId(String(value))}
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder={m.export_choose_track()} />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {document.tracks.map((track, index) => (
                      <SelectItem key={track.id} value={track.id}>
                        {track.language?.toUpperCase() ?? "UND"} · PGS{" "}
                        {index + 1}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel>{m.export_formats()}</FieldLabel>
              <div className="grid grid-cols-2 gap-2">
                {(["json", "srt"] as const).map((format) => (
                  <label
                    key={format}
                    className="flex cursor-pointer items-center gap-3 rounded-xl border p-3"
                  >
                    <Checkbox
                      checked={formats.includes(format)}
                      onCheckedChange={() => toggleFormat(format)}
                    />
                    <span className="font-medium uppercase">{format}</span>
                  </label>
                ))}
              </div>
            </Field>
            <Field>
              <FieldLabel>{m.export_scope()}</FieldLabel>
              <Select
                items={scopeItems}
                value={scope}
                onValueChange={(value) => setScope(value as ExportScope)}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="all_recognized">
                      {m.export_scope_all()}
                    </SelectItem>
                    <SelectItem value="approved_only">
                      {m.export_scope_approved()}
                    </SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel htmlFor="export-name">
                {m.export_base_file_name()}
              </FieldLabel>
              <Input
                id="export-name"
                value={baseName}
                placeholder={document.project.metadata.name}
                onChange={(event) => setBaseName(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel>{m.export_output_folder()}</FieldLabel>
              <Button variant="outline" onClick={() => void chooseOutput()}>
                <FolderOpenIcon />
                {m.common_choose_folder()}
              </Button>
              <FieldDescription>{outputDirectory}</FieldDescription>
            </Field>
          </FieldGroup>
        ) : (
          <div className="flex flex-col gap-3 rounded-xl border p-4">
            <p className="font-medium">{m.export_complete()}</p>
            {result.artifacts.map((artifact) => (
              <div key={artifact.format} className="text-sm">
                <Badge variant="secondary" className="mr-2 uppercase">
                  {artifact.format}
                </Badge>
                <span className="break-all text-muted-foreground">
                  {artifact.path}
                </span>
              </div>
            ))}
          </div>
        )}

        {error && <FieldError>{error}</FieldError>}
        <DialogFooter>
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => handleOpenChange(false)}
          >
            {result ? m.common_done() : m.common_cancel()}
          </Button>
          {!result && (
            <Button
              disabled={
                busy ||
                !resolvedTrackId ||
                !outputDirectory ||
                formats.length === 0
              }
              onClick={() => void startExport()}
            >
              {busy ? <Spinner /> : <DownloadIcon />}
              {m.workspace_export()}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function Metric({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="rounded-lg bg-muted/50 p-3">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 font-medium">{value}</p>
    </div>
  )
}
