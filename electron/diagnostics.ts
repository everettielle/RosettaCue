import { EventEmitter } from "node:events"
import {
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
  type WriteStream,
} from "node:fs"
import { basename, join } from "node:path"

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

export type DiagnosticDraft = Omit<
  DiagnosticEntry,
  "id" | "session_id" | "sequence" | "created_at_ms"
> & {
  id?: string
  created_at_ms?: number
}

const MAX_MEMORY_ENTRIES = 10_000
const MAX_LOG_BYTES = 20 * 1024 * 1024
const RETAINED_SESSION_COUNT = 5

function timestampId() {
  return new Date().toISOString().replaceAll(":", "-").replaceAll(".", "-")
}

function isSecretKey(key: string) {
  return [
    "api_key",
    "apikey",
    "authorization",
    "x_api_key",
    "access_token",
    "refresh_token",
    "password",
    "cookie",
    "set_cookie",
  ].includes(key.toLowerCase().replaceAll("-", "_"))
}

export function sanitizeDiagnosticValue(
  value: unknown,
  key?: string,
  depth = 0
): unknown {
  if (key && isSecretKey(key)) {
    return "[REDACTED]"
  }
  if (depth > 12) {
    return "[MAX_DEPTH]"
  }
  if (typeof value === "string") {
    if (value.startsWith("data:") && value.includes(";base64,")) {
      const encoded = value.split(";base64,", 2)[1] ?? ""
      return {
        redacted: "base64_data",
        estimated_byte_length: Math.floor((encoded.length * 3) / 4),
      }
    }
    return value
  }
  if (Array.isArray(value)) {
    if (value.length > 256 && value.every((item) => typeof item === "number")) {
      return { redacted: "binary_array", byte_length: value.length }
    }
    return value.map((item) => sanitizeDiagnosticValue(item, key, depth + 1))
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([childKey, childValue]) => [
        childKey,
        sanitizeDiagnosticValue(childValue, childKey, depth + 1),
      ])
    )
  }
  return value
}

export class DiagnosticStore extends EventEmitter {
  private readonly directory: string
  private readonly settingsPath: string
  private enabled: boolean
  private sessionId = ""
  private sequence = 0
  private entries: DiagnosticEntry[] = []
  private stream: WriteStream | null = null
  private segment = 0
  private segmentBytes = 0

  constructor(userDataDirectory: string) {
    super()
    this.directory = join(userDataDirectory, "diagnostics")
    this.settingsPath = join(userDataDirectory, "diagnostics.json")
    this.enabled = this.loadEnabled()
    if (this.enabled) {
      this.startSession()
    }
  }

  isEnabled() {
    return this.enabled
  }

  setEnabled(enabled: boolean) {
    if (enabled === this.enabled) {
      return
    }
    if (enabled) {
      this.enabled = true
      this.startSession()
      this.record({
        level: "info",
        source: "electron",
        category: "diagnostics",
        operation: "configure",
        phase: "enabled",
        message: "Debug logging enabled.",
        correlation_id: null,
        duration_ms: null,
        details: {},
      })
    } else {
      this.record({
        level: "info",
        source: "electron",
        category: "diagnostics",
        operation: "configure",
        phase: "disabled",
        message: "Debug logging disabled.",
        correlation_id: null,
        duration_ms: null,
        details: {},
      })
      this.enabled = false
      this.closeStream()
    }
    this.persistEnabled()
    this.emit("enabled-changed", this.enabled)
  }

  record(draft: DiagnosticDraft) {
    if (!this.enabled) {
      return null
    }
    const sequence = ++this.sequence
    const createdAt = draft.created_at_ms ?? Date.now()
    const entry: DiagnosticEntry = {
      ...draft,
      id: draft.id ?? `electron-${createdAt}-${sequence}`,
      session_id: this.sessionId,
      sequence,
      created_at_ms: createdAt,
      details: sanitizeDiagnosticValue(draft.details),
    }
    this.entries.push(entry)
    if (this.entries.length > MAX_MEMORY_ENTRIES) {
      this.entries.splice(0, this.entries.length - MAX_MEMORY_ENTRIES)
    }
    this.write(entry)
    this.emit("entry", entry)
    return entry
  }

  accept(value: unknown) {
    if (!value || typeof value !== "object") {
      return null
    }
    const entry = value as Partial<DiagnosticEntry>
    return this.record({
      id: entry.id,
      created_at_ms: entry.created_at_ms,
      level: entry.level ?? "debug",
      source: entry.source ?? "backend",
      category: entry.category ?? "backend",
      operation: entry.operation ?? "event",
      phase: entry.phase ?? "event",
      message: entry.message ?? "Backend diagnostic event.",
      correlation_id: entry.correlation_id ?? null,
      duration_ms: entry.duration_ms ?? null,
      details: entry.details ?? {},
    })
  }

  snapshot(requestedSessionId?: string) {
    const requested = requestedSessionId?.trim()
    const sessionId =
      requested && this.sessionIds().includes(requested)
        ? requested
        : this.sessionId
    return {
      enabled: this.enabled,
      session_id: sessionId,
      current_session_id: this.sessionId,
      sessions: this.sessionIds(),
      entries:
        sessionId === this.sessionId
          ? [...this.entries]
          : this.readSession(sessionId),
    }
  }

  clear() {
    this.entries = []
    if (this.sessionId) {
      this.closeStream()
      for (const name of readdirSync(this.directory)) {
        if (name.startsWith(`session-${this.sessionId}-`)) {
          rmSync(join(this.directory, name), { force: true })
        }
      }
      this.segment = 0
      if (this.enabled) this.openSegment()
    }
    this.emit("cleared")
  }

  exportSession(destination: string, sessionId?: string) {
    const entries =
      sessionId && sessionId !== this.sessionId
        ? this.readSession(sessionId)
        : this.entries
    const contents = entries.map((entry) => JSON.stringify(entry)).join("\n")
    writeFileSync(destination, contents ? `${contents}\n` : "", "utf8")
  }

  close() {
    this.closeStream()
  }

  private startSession() {
    mkdirSync(this.directory, { recursive: true })
    this.pruneSessions()
    this.sessionId = timestampId()
    this.sequence = 0
    this.entries = []
    this.segment = 0
    this.openSegment()
  }

  private openSegment() {
    this.closeStream()
    this.segment += 1
    this.segmentBytes = 0
    const path = join(
      this.directory,
      `session-${this.sessionId}-${String(this.segment).padStart(3, "0")}.jsonl`
    )
    this.stream = createWriteStream(path, { flags: "a", encoding: "utf8" })
  }

  private write(entry: DiagnosticEntry) {
    const line = `${JSON.stringify(entry)}\n`
    const bytes = Buffer.byteLength(line)
    if (this.segmentBytes > 0 && this.segmentBytes + bytes > MAX_LOG_BYTES) {
      this.openSegment()
    }
    this.segmentBytes += bytes
    this.stream?.write(line)
  }

  private closeStream() {
    this.stream?.end()
    this.stream = null
  }

  private loadEnabled() {
    try {
      const settings = JSON.parse(readFileSync(this.settingsPath, "utf8")) as {
        enabled?: boolean
      }
      return settings.enabled === true
    } catch {
      return false
    }
  }

  private persistEnabled() {
    writeFileSync(
      this.settingsPath,
      `${JSON.stringify({ enabled: this.enabled }, null, 2)}\n`,
      "utf8"
    )
  }

  private pruneSessions() {
    if (!existsSync(this.directory)) {
      return
    }
    const sessions = new Map<string, string[]>()
    for (const name of readdirSync(this.directory)) {
      const match = /^session-(.+)-\d{3}\.jsonl$/.exec(name)
      if (!match) continue
      const files = sessions.get(match[1]) ?? []
      files.push(join(this.directory, basename(name)))
      sessions.set(match[1], files)
    }
    const obsolete = [...sessions.keys()]
      .sort()
      .reverse()
      .slice(RETAINED_SESSION_COUNT - 1)
    for (const session of obsolete) {
      for (const path of sessions.get(session) ?? []) {
        rmSync(path, { force: true })
      }
    }
  }

  private sessionIds() {
    if (!existsSync(this.directory)) return []
    return [
      ...new Set(
        readdirSync(this.directory).flatMap((name) => {
          const match = /^session-(.+)-\d{3}\.jsonl$/.exec(name)
          return match ? [match[1]] : []
        })
      ),
    ]
      .sort()
      .reverse()
  }

  private readSession(sessionId: string) {
    if (!this.sessionIds().includes(sessionId)) return []
    const entries: DiagnosticEntry[] = []
    const files = readdirSync(this.directory)
      .filter((name) => name.startsWith(`session-${sessionId}-`))
      .sort()
    for (const name of files) {
      const lines = readFileSync(join(this.directory, name), "utf8").split("\n")
      for (const line of lines) {
        if (!line.trim()) continue
        try {
          entries.push(JSON.parse(line) as DiagnosticEntry)
        } catch {
          // A partially written final line is ignored until the next snapshot.
        }
      }
    }
    return entries
  }
}
