import { app } from "electron"
import { randomUUID } from "node:crypto"
import { EventEmitter } from "node:events"
import { access } from "node:fs/promises"
import { createInterface } from "node:readline"
import { join } from "node:path"
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process"

import type { BackendEvent, BackendMethod } from "./contracts"
import type { DiagnosticDraft } from "./diagnostics"

type PendingRequest = {
  resolve: (value: unknown) => void
  reject: (reason: Error) => void
  method: BackendMethod
  startedAt: number
}

type BackendResponse = {
  id: string
  result?: unknown
  error?: {
    code: string
    message: string
  }
}

type BackendEventMessage = {
  event: BackendEvent
  payload: unknown
}

function executableName() {
  return process.platform === "win32"
    ? "rosettacue-backend.exe"
    : "rosettacue-backend"
}

function backendPath() {
  if (process.env.ROSETTACUE_BACKEND_PATH) {
    return process.env.ROSETTACUE_BACKEND_PATH
  }

  if (app.isPackaged) {
    return join(process.resourcesPath, "backend", executableName())
  }

  return join(app.getAppPath(), "target", "debug", executableName())
}

function mediaToolsPath() {
  if (process.env.ROSETTACUE_MEDIA_TOOLS_DIR) {
    return process.env.ROSETTACUE_MEDIA_TOOLS_DIR
  }

  return app.isPackaged
    ? join(process.resourcesPath, "tools")
    : join(app.getAppPath(), "resources", "tools")
}

export class RustBackend extends EventEmitter {
  private child: ChildProcessWithoutNullStreams | null = null
  private readonly pending = new Map<string, PendingRequest>()
  private readonly report: (entry: DiagnosticDraft) => void
  private readonly diagnosticsEnabled: () => boolean

  constructor(
    report: (entry: DiagnosticDraft) => void,
    diagnosticsEnabled: () => boolean
  ) {
    super()
    this.report = report
    this.diagnosticsEnabled = diagnosticsEnabled
  }

  async start() {
    if (this.child) {
      return
    }

    const command = backendPath()
    await access(command)
    const startedAt = Date.now()
    this.report({
      level: "info",
      source: "electron",
      category: "process",
      operation: "rust_backend",
      phase: "start",
      message: "Starting Rust backend.",
      correlation_id: null,
      duration_ms: null,
      details: { command, media_tools_directory: mediaToolsPath() },
    })
    const child = spawn(command, [], {
      cwd: app.getPath("userData"),
      env: {
        ...process.env,
        ROSETTACUE_MEDIA_TOOLS_DIR: mediaToolsPath(),
        ROSETTACUE_DEBUG_LOGGING: this.diagnosticsEnabled() ? "1" : "0",
      },
      shell: false,
      stdio: ["pipe", "pipe", "pipe"],
    })
    this.child = child

    const lines = createInterface({ input: child.stdout })
    lines.on("line", (line) => this.handleLine(line))
    child.stderr.setEncoding("utf8")
    child.stderr.on("data", (message: string) => {
      console.error(`[rosettacue-backend] ${message.trimEnd()}`)
      this.report({
        level: "warn",
        source: "backend",
        category: "process",
        operation: "stderr",
        phase: "output",
        message: "Rust backend wrote to stderr.",
        correlation_id: null,
        duration_ms: null,
        details: { output: message.trimEnd() },
      })
    })
    child.once("error", (error) => {
      this.report({
        level: "error",
        source: "electron",
        category: "process",
        operation: "rust_backend",
        phase: "error",
        message: "Rust backend process failed.",
        correlation_id: null,
        duration_ms: Date.now() - startedAt,
        details: { error: error.message },
      })
      this.failAll(error)
    })
    child.once("exit", (code, signal) => {
      this.child = null
      this.report({
        level: code === 0 ? "info" : "error",
        source: "electron",
        category: "process",
        operation: "rust_backend",
        phase: "exit",
        message: "Rust backend process exited.",
        correlation_id: null,
        duration_ms: null,
        details: { code, signal },
      })
      this.failAll(
        new Error(
          `Rust backend exited${code === null ? "" : ` with code ${code}`}${signal ? ` (${signal})` : ""}.`
        )
      )
    })

    await new Promise<void>((resolve, reject) => {
      child.once("spawn", resolve)
      child.once("error", reject)
    })
    this.report({
      level: "info",
      source: "electron",
      category: "process",
      operation: "rust_backend",
      phase: "ready",
      message: "Rust backend started.",
      correlation_id: null,
      duration_ms: Date.now() - startedAt,
      details: { pid: child.pid },
    })
  }

  invoke(method: BackendMethod, params: unknown = {}) {
    if (!this.child?.stdin.writable) {
      return Promise.reject(new Error("Rust backend is not running."))
    }

    const id = randomUUID()
    const startedAt = Date.now()
    this.report({
      level: "debug",
      source: "electron",
      category: "ipc",
      operation: method,
      phase: "request",
      message: `Invoking backend method ${method}.`,
      correlation_id: id,
      duration_ms: null,
      details: { params },
    })
    return new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject, method, startedAt })
      this.child?.stdin.write(
        `${JSON.stringify({ id, method, params })}\n`,
        (error) => {
          if (!error) {
            return
          }
          this.pending.delete(id)
          this.report({
            level: "error",
            source: "electron",
            category: "ipc",
            operation: method,
            phase: "write_failed",
            message: `Unable to send backend method ${method}.`,
            correlation_id: id,
            duration_ms: Date.now() - startedAt,
            details: { error: error.message },
          })
          reject(error)
        }
      )
    })
  }

  stop() {
    const child = this.child
    this.child = null
    child?.kill()
    this.failAll(new Error("Rust backend stopped."))
  }

  private handleLine(line: string) {
    let message: BackendResponse | BackendEventMessage
    try {
      message = JSON.parse(line) as BackendResponse | BackendEventMessage
    } catch (error) {
      console.error("Ignored malformed Rust backend message.", error)
      return
    }

    if ("event" in message) {
      this.emit("backend-event", message.event, message.payload)
      return
    }

    const pending = this.pending.get(message.id)
    if (!pending) {
      return
    }
    this.pending.delete(message.id)
    if (message.error) {
      this.report({
        level: "error",
        source: "electron",
        category: "ipc",
        operation: pending.method,
        phase: "failed",
        message: `Backend method ${pending.method} failed.`,
        correlation_id: message.id,
        duration_ms: Date.now() - pending.startedAt,
        details: { error: message.error },
      })
      pending.reject(new Error(message.error.message))
      return
    }
    this.report({
      level: "debug",
      source: "electron",
      category: "ipc",
      operation: pending.method,
      phase: "completed",
      message: `Backend method ${pending.method} completed.`,
      correlation_id: message.id,
      duration_ms: Date.now() - pending.startedAt,
      details: { result: message.result },
    })
    pending.resolve(message.result)
  }

  private failAll(error: Error) {
    for (const [id, request] of this.pending) {
      this.report({
        level: "error",
        source: "electron",
        category: "ipc",
        operation: request.method,
        phase: "interrupted",
        message: `Backend method ${request.method} was interrupted.`,
        correlation_id: id,
        duration_ms: Date.now() - request.startedAt,
        details: { error: error.message },
      })
      request.reject(error)
    }
    this.pending.clear()
  }
}
