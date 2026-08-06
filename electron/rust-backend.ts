import { app } from "electron"
import { randomUUID } from "node:crypto"
import { EventEmitter } from "node:events"
import { access } from "node:fs/promises"
import { createInterface } from "node:readline"
import { join } from "node:path"
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process"

import type { BackendEvent, BackendMethod } from "./contracts"

type PendingRequest = {
  resolve: (value: unknown) => void
  reject: (reason: Error) => void
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

  async start() {
    if (this.child) {
      return
    }

    const command = backendPath()
    await access(command)
    const child = spawn(command, [], {
      cwd: app.getPath("userData"),
      env: {
        ...process.env,
        ROSETTACUE_MEDIA_TOOLS_DIR: mediaToolsPath(),
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
    })
    child.once("error", (error) => this.failAll(error))
    child.once("exit", (code, signal) => {
      this.child = null
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
  }

  invoke(method: BackendMethod, params: unknown = {}) {
    if (!this.child?.stdin.writable) {
      return Promise.reject(new Error("Rust backend is not running."))
    }

    const id = randomUUID()
    return new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
      this.child?.stdin.write(
        `${JSON.stringify({ id, method, params })}\n`,
        (error) => {
          if (!error) {
            return
          }
          this.pending.delete(id)
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
      pending.reject(new Error(message.error.message))
      return
    }
    pending.resolve(message.result)
  }

  private failAll(error: Error) {
    for (const request of this.pending.values()) {
      request.reject(error)
    }
    this.pending.clear()
  }
}
