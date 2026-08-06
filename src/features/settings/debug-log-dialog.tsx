import { BugIcon, Trash2Icon } from "lucide-react"

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
import type {
  OcrDebugLogEntry,
  OcrDebugStatus,
} from "@/features/projects/types"
import * as m from "@/paraglide/messages.js"
import { getLocale } from "@/paraglide/runtime.js"

function statusLabel(status: OcrDebugStatus) {
  const labels: Record<OcrDebugStatus, () => string> = {
    succeeded: m.debug_log_status_succeeded,
    validation_failed: m.debug_log_status_validation_failed,
    provider_failed: m.debug_log_status_provider_failed,
  }
  return labels[status]()
}

function stageLabel(stage: string) {
  if (stage === "recognition") return m.debug_log_stage_recognition()
  if (stage === "style") return m.debug_log_stage_style()
  return stage
}

function timestamp(createdAtMs: number) {
  return new Intl.DateTimeFormat(getLocale(), {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(new Date(createdAtMs))
}

export function DebugLogDialog({
  open,
  entries,
  onOpenChange,
  onClear,
}: {
  open: boolean
  entries: OcrDebugLogEntry[]
  onOpenChange: (open: boolean) => void
  onClear: () => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[min(760px,calc(100vh-3rem))] max-h-[calc(100vh-3rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl">
        <DialogHeader className="border-b px-6 pt-6 pb-4">
          <DialogTitle>{m.debug_log_title()}</DialogTitle>
          <DialogDescription>{m.debug_log_description()}</DialogDescription>
        </DialogHeader>

        <ScrollArea className="min-h-0 flex-1">
          <div className="grid gap-3 p-6">
            {entries.length === 0 ? (
              <Empty className="min-h-72 border">
                <EmptyHeader>
                  <EmptyMedia variant="icon">
                    <BugIcon />
                  </EmptyMedia>
                  <EmptyTitle>{m.debug_log_empty()}</EmptyTitle>
                  <EmptyDescription>
                    {m.debug_log_empty_description()}
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : (
              [...entries].reverse().map((entry) => (
                <Card
                  key={`${entry.created_at_ms}-${entry.cue_id}-${entry.stage}-${entry.attempt}`}
                  size="sm"
                >
                  <CardHeader>
                    <CardTitle>{stageLabel(entry.stage)}</CardTitle>
                    <CardDescription>
                      {timestamp(entry.created_at_ms)} ·{" "}
                      {m.debug_log_entry_description({
                        cue: entry.cue_index,
                        provider: entry.provider,
                        model: entry.model,
                      })}
                    </CardDescription>
                    <CardAction>
                      <Badge
                        variant={
                          entry.status === "succeeded"
                            ? "secondary"
                            : "destructive"
                        }
                      >
                        {statusLabel(entry.status)}
                      </Badge>
                    </CardAction>
                  </CardHeader>
                  <CardContent className="grid gap-3">
                    {entry.error && (
                      <Alert variant="destructive">
                        <AlertTitle>{statusLabel(entry.status)}</AlertTitle>
                        <AlertDescription className="break-words">
                          {entry.error}
                        </AlertDescription>
                      </Alert>
                    )}
                    <div className="grid gap-1.5">
                      <p className="text-xs font-medium text-muted-foreground">
                        {m.debug_log_raw_response()}
                      </p>
                      <ScrollArea className="max-h-72 rounded-xl bg-muted">
                        <pre className="p-3 font-mono text-xs break-words whitespace-pre-wrap">
                          {entry.raw_response ?? m.debug_log_no_response()}
                        </pre>
                      </ScrollArea>
                    </div>
                  </CardContent>
                  <CardFooter className="border-t text-xs text-muted-foreground">
                    {m.debug_log_attempt({ attempt: entry.attempt })}
                  </CardFooter>
                </Card>
              ))
            )}
          </div>
        </ScrollArea>

        <DialogFooter className="border-t px-6 py-4 sm:justify-between">
          <Button
            variant="outline"
            disabled={entries.length === 0}
            onClick={onClear}
          >
            <Trash2Icon data-icon="inline-start" />
            {m.debug_log_clear()}
          </Button>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {m.debug_log_back_to_settings()}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
