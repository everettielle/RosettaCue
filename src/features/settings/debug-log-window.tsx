import * as React from "react"
import {
  BugIcon,
  DownloadIcon,
  PauseIcon,
  PlayIcon,
  Trash2Icon,
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
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Toaster, toast } from "@/components/ui/toast"
import { desktop } from "@/lib/desktop"
import * as m from "@/paraglide/messages.js"
import { getLocale } from "@/paraglide/runtime.js"
import type {
  DiagnosticEntry,
  DiagnosticLevel,
  DiagnosticSnapshot,
} from "@/types/desktop"

type DetailRecord = Record<string, unknown>

function asRecord(value: unknown): DetailRecord {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as DetailRecord)
    : {}
}

function timestamp(value: number) {
  return new Intl.DateTimeFormat(getLocale(), {
    dateStyle: "short",
    timeStyle: "medium",
  }).format(new Date(value))
}

function displayValue(value: unknown) {
  if (value === undefined || value === null) return m.debug_log_no_data()
  if (typeof value === "string") {
    try {
      return JSON.stringify(JSON.parse(value), null, 2)
    } catch {
      return value
    }
  }
  return JSON.stringify(value, null, 2)
}

function levelVariant(level: DiagnosticLevel) {
  if (level === "error") return "destructive" as const
  if (level === "warn") return "outline" as const
  if (level === "debug") return "secondary" as const
  return "default" as const
}

function JsonPanel({ value }: { value: unknown }) {
  return (
    <ScrollArea className="h-full rounded-xl bg-muted">
      <pre className="p-4 font-mono text-xs break-words whitespace-pre-wrap">
        {displayValue(value)}
      </pre>
    </ScrollArea>
  )
}

function EntryDetail({ entry }: { entry: DiagnosticEntry | null }) {
  if (!entry) {
    return (
      <Empty className="h-full">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <BugIcon />
          </EmptyMedia>
          <EmptyTitle>{m.debug_log_no_selection()}</EmptyTitle>
        </EmptyHeader>
      </Empty>
    )
  }
  const details = asRecord(entry.details)
  const request = details.request ?? details.params
  const response = details.response ?? details.result
  const error = details.error
  return (
    <div className="flex h-full min-h-0 flex-col gap-4 p-5">
      <div className="flex shrink-0 items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="truncate font-medium">{entry.operation}</h2>
            <Badge variant={levelVariant(entry.level)}>{entry.level}</Badge>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">{entry.message}</p>
        </div>
        <div className="text-right text-xs text-muted-foreground">
          <p>{timestamp(entry.created_at_ms)}</p>
          {entry.duration_ms !== null && (
            <p>{m.debug_log_duration({ duration: entry.duration_ms })}</p>
          )}
        </div>
      </div>
      <Separator />
      <Tabs defaultValue="overview" className="min-h-0 flex-1">
        <TabsList>
          <TabsTrigger value="overview">{m.debug_log_overview()}</TabsTrigger>
          <TabsTrigger value="details">{m.debug_log_details()}</TabsTrigger>
          <TabsTrigger value="request">{m.debug_log_request()}</TabsTrigger>
          <TabsTrigger value="response">{m.debug_log_response()}</TabsTrigger>
          <TabsTrigger value="error">{m.debug_log_error()}</TabsTrigger>
        </TabsList>
        <TabsContent value="overview" className="min-h-0">
          <JsonPanel
            value={{
              id: entry.id,
              sequence: entry.sequence,
              source: entry.source,
              category: entry.category,
              operation: entry.operation,
              phase: entry.phase,
              level: entry.level,
              correlation_id: entry.correlation_id,
              duration_ms: entry.duration_ms,
            }}
          />
        </TabsContent>
        <TabsContent value="details" className="min-h-0">
          <JsonPanel value={entry.details} />
        </TabsContent>
        <TabsContent value="request" className="min-h-0">
          <JsonPanel value={request} />
        </TabsContent>
        <TabsContent value="response" className="min-h-0">
          <JsonPanel value={response} />
        </TabsContent>
        <TabsContent value="error" className="min-h-0">
          <JsonPanel value={error} />
        </TabsContent>
      </Tabs>
    </div>
  )
}

export function DebugLogWindow() {
  const [entries, setEntries] = React.useState<DiagnosticEntry[]>([])
  const [enabled, setEnabled] = React.useState(false)
  const [sessionId, setSessionId] = React.useState("")
  const [currentSessionId, setCurrentSessionId] = React.useState("")
  const [sessions, setSessions] = React.useState<string[]>([])
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState("")
  const [level, setLevel] = React.useState("all")
  const [source, setSource] = React.useState("all")
  const [paused, setPaused] = React.useState(false)
  const pausedRef = React.useRef(false)
  const sessionRef = React.useRef("")
  const currentSessionRef = React.useRef("")

  const applySnapshot = React.useCallback((snapshot: DiagnosticSnapshot) => {
    setEntries(snapshot.entries)
    setEnabled(snapshot.enabled)
    setSessionId(snapshot.session_id)
    setCurrentSessionId(snapshot.current_session_id)
    setSessions(snapshot.sessions)
    sessionRef.current = snapshot.session_id
    currentSessionRef.current = snapshot.current_session_id
    setSelectedId((current) =>
      current && snapshot.entries.some((entry) => entry.id === current)
        ? current
        : (snapshot.entries.at(-1)?.id ?? null)
    )
  }, [])
  const refresh = React.useCallback(
    async (requestedSessionId?: string) => {
      applySnapshot(await desktop.diagnostics.snapshot(requestedSessionId))
    },
    [applySnapshot]
  )

  React.useEffect(() => {
    void desktop.diagnostics.snapshot().then(applySnapshot)
    const removeEntry = desktop.diagnostics.onEntry((entry) => {
      if (
        pausedRef.current ||
        sessionRef.current !== currentSessionRef.current
      ) {
        return
      }
      setEntries((current) => [...current.slice(-9_999), entry])
      setSelectedId((current) => current ?? entry.id)
    })
    const removeEnabled = desktop.diagnostics.onEnabledChange(setEnabled)
    const removeCleared = desktop.diagnostics.onCleared(() => {
      setEntries([])
      setSelectedId(null)
    })
    return () => {
      removeEntry()
      removeEnabled()
      removeCleared()
    }
  }, [applySnapshot])

  const sources = React.useMemo(
    () => [...new Set(entries.map((entry) => entry.source))].sort(),
    [entries]
  )
  const filtered = React.useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    return [...entries]
      .reverse()
      .filter((entry) => level === "all" || entry.level === level)
      .filter((entry) => source === "all" || entry.source === source)
      .filter(
        (entry) =>
          !normalized ||
          entry.message.toLocaleLowerCase().includes(normalized) ||
          entry.operation.toLocaleLowerCase().includes(normalized) ||
          entry.category.toLocaleLowerCase().includes(normalized) ||
          JSON.stringify(entry.details).toLocaleLowerCase().includes(normalized)
      )
  }, [entries, level, query, source])
  const selected =
    entries.find((entry) => entry.id === selectedId) ?? filtered[0] ?? null

  const togglePause = () => {
    const next = !paused
    pausedRef.current = next
    setPaused(next)
    if (!next) void refresh(sessionRef.current)
  }

  return (
    <main className="flex h-screen min-h-0 flex-col bg-background text-foreground">
      <header className="flex shrink-0 items-start gap-4 border-b px-5 py-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h1 className="text-lg font-semibold">{m.debug_log_title()}</h1>
            <Badge variant="secondary">
              {m.debug_log_entries({ count: entries.length })}
            </Badge>
          </div>
          <p className="text-sm text-muted-foreground">
            {m.debug_log_description()}
          </p>
          {sessionId && (
            <p className="mt-1 font-mono text-xs text-muted-foreground">
              {m.debug_log_session({ session: sessionId })}
            </p>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={togglePause}>
            {paused ? (
              <PlayIcon data-icon="inline-start" />
            ) : (
              <PauseIcon data-icon="inline-start" />
            )}
            {paused ? m.debug_log_resume() : m.debug_log_pause()}
          </Button>
          <Button
            variant="outline"
            disabled={entries.length === 0}
            onClick={() =>
              void desktop.diagnostics.exportCurrent(sessionId).then((path) => {
                if (path) {
                  toast.add({
                    title: m.debug_log_export(),
                    description: m.debug_log_exported({ path }),
                    type: "success",
                  })
                }
              })
            }
          >
            <DownloadIcon data-icon="inline-start" />
            {m.debug_log_export()}
          </Button>
          <Button
            variant="outline"
            disabled={entries.length === 0 || sessionId !== currentSessionId}
            onClick={() => void desktop.diagnostics.clear()}
          >
            <Trash2Icon data-icon="inline-start" />
            {m.debug_log_clear()}
          </Button>
        </div>
      </header>

      {!enabled && (
        <Alert className="mx-5 mt-4 shrink-0">
          <BugIcon />
          <AlertTitle>{m.debug_log_disabled_title()}</AlertTitle>
          <AlertDescription>
            {m.debug_log_disabled_description()}
          </AlertDescription>
        </Alert>
      )}

      <FieldGroup className="shrink-0 grid-cols-[minmax(0,1fr)_160px_180px_240px] gap-3 border-b p-4 md:grid">
        <Field>
          <FieldLabel className="sr-only" htmlFor="debug-search">
            {m.debug_log_search()}
          </FieldLabel>
          <Input
            id="debug-search"
            value={query}
            placeholder={m.debug_log_search()}
            onChange={(event) => setQuery(event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel className="sr-only">
            {m.debug_log_all_levels()}
          </FieldLabel>
          <Select
            value={level}
            onValueChange={(value) => setLevel(value ?? "all")}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false}>
              <SelectGroup>
                <SelectItem value="all">{m.debug_log_all_levels()}</SelectItem>
                <SelectItem value="debug">Debug</SelectItem>
                <SelectItem value="info">Info</SelectItem>
                <SelectItem value="warn">Warn</SelectItem>
                <SelectItem value="error">Error</SelectItem>
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel className="sr-only">
            {m.debug_log_session({ session: sessionId })}
          </FieldLabel>
          <Select
            value={sessionId}
            onValueChange={(value) => {
              if (value) {
                void refresh(value)
              }
            }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false}>
              <SelectGroup>
                {sessions.map((item) => (
                  <SelectItem key={item} value={item}>
                    {item === currentSessionId
                      ? m.debug_log_current_session({ session: item })
                      : item}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel className="sr-only">
            {m.debug_log_all_sources()}
          </FieldLabel>
          <Select
            value={source}
            onValueChange={(value) => setSource(value ?? "all")}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false}>
              <SelectGroup>
                <SelectItem value="all">{m.debug_log_all_sources()}</SelectItem>
                {sources.map((item) => (
                  <SelectItem key={item} value={item}>
                    {item}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      </FieldGroup>

      <div className="grid min-h-0 flex-1 grid-cols-[380px_minmax(0,1fr)]">
        <ScrollArea className="min-h-0 border-r">
          {filtered.length === 0 ? (
            <Empty className="min-h-80">
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
            <div className="flex flex-col gap-1 p-2">
              {filtered.map((entry) => (
                <Button
                  key={entry.id}
                  variant={selected?.id === entry.id ? "secondary" : "ghost"}
                  className="h-auto w-full items-start justify-start px-3 py-2 text-left whitespace-normal"
                  onClick={() => setSelectedId(entry.id)}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <Badge variant={levelVariant(entry.level)}>
                        {entry.level}
                      </Badge>
                      <span className="truncate font-mono text-xs">
                        {entry.source} · {entry.operation}
                      </span>
                    </div>
                    <p className="mt-1 line-clamp-2 text-xs font-normal text-muted-foreground">
                      {entry.message}
                    </p>
                    <p className="mt-1 text-xs font-normal text-muted-foreground">
                      {timestamp(entry.created_at_ms)} · {entry.phase}
                    </p>
                  </div>
                </Button>
              ))}
            </div>
          )}
        </ScrollArea>
        <section className="min-h-0 min-w-0">
          <EntryDetail entry={selected} />
        </section>
      </div>
      <Toaster />
    </main>
  )
}
