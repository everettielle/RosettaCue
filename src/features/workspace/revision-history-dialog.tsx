import * as React from "react"
import { HistoryIcon, RotateCcwIcon, Trash2Icon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Spinner } from "@/components/ui/spinner"
import type { CueRevision, SubtitleCue } from "@/features/projects/types"
import { RenderedSubtitle } from "@/features/workspace/rendered-subtitle"
import { desktop } from "@/lib/desktop"
import * as m from "@/paraglide/messages.js"
import { getLocale } from "@/paraglide/runtime.js"

function revisionAuthorLabel(author: CueRevision["author"]) {
  const labels: Record<CueRevision["author"], () => string> = {
    ocr: m.revision_author_ocr,
    human: m.revision_author_human,
    translation: m.revision_author_translation,
  }
  return labels[author]()
}

export function RevisionHistoryDialog({
  open,
  projectPath,
  cue,
  onOpenChange,
  onChanged,
}: {
  open: boolean
  projectPath: string
  cue: SubtitleCue | null
  onOpenChange: (open: boolean) => void
  onChanged: (message: string) => Promise<void>
}) {
  const [history, setHistory] = React.useState<CueRevision[]>([])
  const [loadedCueId, setLoadedCueId] = React.useState<string | null>(null)
  const [busyRevisionId, setBusyRevisionId] = React.useState<string | null>(
    null
  )
  const [pendingDelete, setPendingDelete] = React.useState<CueRevision | null>(
    null
  )
  const [error, setError] = React.useState<string | null>(null)
  const loading = Boolean(open && cue && loadedCueId !== cue.id)

  const loadHistory = React.useCallback(async () => {
    if (!cue) return
    try {
      const revisions = await desktop.invoke<CueRevision[]>(
        "cue_revision_history",
        { projectPath, cueId: cue.id }
      )
      setHistory(revisions)
      setLoadedCueId(cue.id)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      setLoadedCueId(cue.id)
    }
  }, [cue, projectPath])

  React.useEffect(() => {
    if (!open || !cue) return
    let active = true
    desktop
      .invoke<CueRevision[]>("cue_revision_history", {
        projectPath,
        cueId: cue.id,
      })
      .then((revisions) => {
        if (!active) return
        setHistory(revisions)
        setLoadedCueId(cue.id)
      })
      .catch((reason: unknown) => {
        if (!active) return
        setError(reason instanceof Error ? reason.message : String(reason))
        setLoadedCueId(cue.id)
      })
    return () => {
      active = false
    }
  }, [cue, open, projectPath])

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setPendingDelete(null)
      setError(null)
    }
    onOpenChange(nextOpen)
  }

  const restoreRevision = async (revision: CueRevision) => {
    if (!cue) return
    setBusyRevisionId(revision.id)
    setError(null)
    try {
      await desktop.invoke("restore_cue_revision", {
        projectPath,
        cueId: cue.id,
        revisionId: revision.id,
      })
      await loadHistory()
      await onChanged(m.status_revision_restored())
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusyRevisionId(null)
    }
  }

  const deleteRevision = async () => {
    if (!cue || !pendingDelete) return
    setBusyRevisionId(pendingDelete.id)
    setError(null)
    try {
      const revisions = await desktop.invoke<CueRevision[]>(
        "delete_cue_revision",
        {
          projectPath,
          cueId: cue.id,
          revisionId: pendingDelete.id,
        }
      )
      setHistory(revisions)
      setPendingDelete(null)
      await onChanged(m.status_revision_deleted())
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusyRevisionId(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{m.revision_history_title()}</DialogTitle>
          <DialogDescription>
            {cue
              ? m.revision_history_description({ index: cue.cue_index })
              : m.revision_history_no_cue()}
          </DialogDescription>
        </DialogHeader>

        {error && (
          <Alert variant="destructive">
            <AlertTitle>{m.workspace_action_failed()}</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        <ScrollArea className="max-h-[55vh] min-h-48">
          {loading ? (
            <div className="flex min-h-48 items-center justify-center">
              <Spinner />
            </div>
          ) : history.length === 0 ? (
            <Empty className="border-0">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HistoryIcon />
                </EmptyMedia>
                <EmptyTitle>{m.revision_history_empty()}</EmptyTitle>
                <EmptyDescription>
                  {m.revision_history_empty_description()}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="flex flex-col gap-3 p-1 pr-3">
              {history.map((revision, index) => {
                const busy = busyRevisionId === revision.id
                const confirmingDelete = pendingDelete?.id === revision.id
                return (
                  <Card key={revision.id} size="sm">
                    <CardHeader>
                      <CardTitle className="flex items-center gap-2 text-sm">
                        {revisionAuthorLabel(revision.author)}
                        {index === 0 && (
                          <Badge variant="secondary">
                            {m.revision_current()}
                          </Badge>
                        )}
                      </CardTitle>
                      <CardDescription>
                        {new Intl.DateTimeFormat(getLocale(), {
                          dateStyle: "medium",
                          timeStyle: "medium",
                        }).format(new Date(revision.created_at))}
                      </CardDescription>
                      <CardAction>
                        <Badge variant="outline">
                          {revision.document.subtitle.language.toUpperCase()}
                        </Badge>
                      </CardAction>
                    </CardHeader>
                    <CardContent className="py-1">
                      <RenderedSubtitle
                        document={revision.document.subtitle}
                        fontSize={16}
                        lineHeight={32}
                        appearance="document"
                        wrap
                      />
                      {confirmingDelete && (
                        <Alert variant="destructive" className="mt-3">
                          <AlertTitle>
                            {m.revision_delete_confirm_title()}
                          </AlertTitle>
                          <AlertDescription>
                            {m.revision_delete_confirm_description()}
                          </AlertDescription>
                        </Alert>
                      )}
                    </CardContent>
                    <CardFooter className="justify-end gap-2 border-t">
                      {confirmingDelete ? (
                        <>
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={Boolean(busyRevisionId)}
                            onClick={() => setPendingDelete(null)}
                          >
                            {m.common_cancel()}
                          </Button>
                          <Button
                            variant="destructive"
                            size="sm"
                            disabled={Boolean(busyRevisionId)}
                            onClick={() => void deleteRevision()}
                          >
                            {busy && <Spinner data-icon="inline-start" />}
                            {m.common_delete()}
                          </Button>
                        </>
                      ) : (
                        <>
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={Boolean(busyRevisionId)}
                            onClick={() => void restoreRevision(revision)}
                          >
                            {busy && <Spinner data-icon="inline-start" />}
                            {!busy && (
                              <RotateCcwIcon data-icon="inline-start" />
                            )}
                            {m.revision_restore()}
                          </Button>
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={
                              history.length <= 1 || Boolean(busyRevisionId)
                            }
                            onClick={() => setPendingDelete(revision)}
                          >
                            <Trash2Icon data-icon="inline-start" />
                            {m.common_delete()}
                          </Button>
                        </>
                      )}
                    </CardFooter>
                  </Card>
                )
              })}
            </div>
          )}
        </ScrollArea>

        <DialogFooter showCloseButton />
      </DialogContent>
    </Dialog>
  )
}
