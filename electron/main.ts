import {
  app,
  BrowserWindow,
  dialog,
  ipcMain,
  nativeTheme,
  screen,
  shell,
  type OpenDialogOptions,
} from "electron"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import {
  backendEvents,
  backendMethods,
  type BackendEvent,
  type BackendMethod,
  type WindowMode,
} from "./contracts"
import { DiagnosticStore } from "./diagnostics"
import { RustBackend } from "./rust-backend"
import * as m from "../src/paraglide/messages.js"

const currentDirectory = dirname(fileURLToPath(import.meta.url))
const appRoot = join(currentDirectory, "..")
const rendererDirectory = join(appRoot, "dist")
const allowedMethods = new Set<string>(backendMethods)
const allowedEvents = new Set<string>(backendEvents)

let mainWindow: BrowserWindow | null = null
let debugWindow: BrowserWindow | null = null
let diagnostics: DiagnosticStore | null = null
const backend = new RustBackend(
  (entry) => diagnostics?.record(entry),
  () => diagnostics?.isEnabled() ?? false
)

const welcomeWindowSize = { width: 960, height: 600 }
const workspaceWindowSize = { width: 1440, height: 900 }

function recordElectronOperation(
  operation: string,
  phase: string,
  message: string,
  details: unknown,
  durationMs: number | null = null
) {
  diagnostics?.record({
    level: phase === "failed" ? "error" : "debug",
    source: "electron",
    category: "desktop",
    operation,
    phase,
    message,
    correlation_id: null,
    duration_ms: durationMs,
    details,
  })
}

function fitToWorkArea(width: number, height: number) {
  const workArea = screen.getPrimaryDisplay().workAreaSize
  return {
    width: Math.min(width, Math.max(880, workArea.width - 64)),
    height: Math.min(height, Math.max(560, workArea.height - 64)),
  }
}

function setWindowMode(mode: WindowMode) {
  if (!mainWindow) {
    return
  }

  if (mainWindow.isFullScreen()) {
    mainWindow.setFullScreen(false)
  }
  if (mainWindow.isMaximized()) {
    mainWindow.unmaximize()
  }

  if (mode === "welcome") {
    mainWindow.setResizable(false)
    mainWindow.setMaximizable(false)
    mainWindow.setMinimumSize(welcomeWindowSize.width, welcomeWindowSize.height)
    mainWindow.setSize(welcomeWindowSize.width, welcomeWindowSize.height, true)
    mainWindow.center()
    return
  }

  const size = fitToWorkArea(
    workspaceWindowSize.width,
    workspaceWindowSize.height
  )
  mainWindow.setMinimumSize(1080, 700)
  mainWindow.setResizable(true)
  mainWindow.setMaximizable(true)
  mainWindow.setSize(size.width, size.height, true)
  mainWindow.center()
}

function createWindow() {
  mainWindow = new BrowserWindow({
    title: "RosettaCue",
    width: welcomeWindowSize.width,
    height: welcomeWindowSize.height,
    minWidth: welcomeWindowSize.width,
    minHeight: welcomeWindowSize.height,
    resizable: false,
    maximizable: false,
    show: false,
    backgroundColor: nativeTheme.shouldUseDarkColors ? "#171717" : "#ffffff",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "default",
    trafficLightPosition:
      process.platform === "darwin" ? { x: 18, y: 22 } : undefined,
    webPreferences: {
      preload: join(currentDirectory, "preload.mjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  })

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("https://") || url.startsWith("http://")) {
      void shell.openExternal(url)
    }
    return { action: "deny" }
  })
  mainWindow.webContents.on("will-navigate", (event, url) => {
    const currentUrl = mainWindow?.webContents.getURL()
    if (currentUrl && url !== currentUrl) {
      event.preventDefault()
    }
  })
  mainWindow.once("ready-to-show", () => mainWindow?.show())
  mainWindow.on("closed", () => {
    mainWindow = null
  })

  if (process.env.VITE_DEV_SERVER_URL) {
    void mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL)
  } else {
    void mainWindow.loadFile(join(rendererDirectory, "index.html"))
  }
}

function loadRenderer(window: BrowserWindow, view?: string) {
  if (process.env.VITE_DEV_SERVER_URL) {
    const url = new URL(process.env.VITE_DEV_SERVER_URL)
    if (view) url.searchParams.set("view", view)
    void window.loadURL(url.toString())
  } else {
    void window.loadFile(join(rendererDirectory, "index.html"), {
      query: view ? { view } : undefined,
    })
  }
}

function createDebugWindow() {
  if (debugWindow && !debugWindow.isDestroyed()) {
    debugWindow.show()
    debugWindow.focus()
    return
  }
  debugWindow = new BrowserWindow({
    title: "RosettaCue Debug Log",
    width: 1120,
    height: 760,
    minWidth: 760,
    minHeight: 520,
    show: false,
    backgroundColor: nativeTheme.shouldUseDarkColors ? "#171717" : "#ffffff",
    webPreferences: {
      preload: join(currentDirectory, "preload.mjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  })
  recordElectronOperation(
    "debug_window",
    "opened",
    "Debug Log window opened.",
    {}
  )
  debugWindow.webContents.setWindowOpenHandler(() => ({ action: "deny" }))
  debugWindow.once("ready-to-show", () => debugWindow?.show())
  debugWindow.on("closed", () => {
    debugWindow = null
  })
  loadRenderer(debugWindow, "diagnostics")
}

function registerIpc() {
  ipcMain.handle(
    "rosettacue:backend:invoke",
    (_event, method: BackendMethod, params: unknown) => {
      if (!allowedMethods.has(method)) {
        throw new Error(`Backend method is not allowed: ${method}`)
      }
      return backend.invoke(method, params)
    }
  )

  ipcMain.handle(
    "rosettacue:dialog:directory",
    async (_event, options?: { title?: string; defaultPath?: string }) => {
      const startedAt = Date.now()
      recordElectronOperation(
        "select_directory",
        "opened",
        "Directory picker opened.",
        { options }
      )
      const dialogOptions: OpenDialogOptions = {
        title: options?.title,
        defaultPath: options?.defaultPath,
        properties: ["openDirectory", "createDirectory"],
      }
      const result = mainWindow
        ? await dialog.showOpenDialog(mainWindow, dialogOptions)
        : await dialog.showOpenDialog(dialogOptions)
      const path = result.canceled ? null : result.filePaths[0]
      recordElectronOperation(
        "select_directory",
        result.canceled ? "canceled" : "completed",
        "Directory picker closed.",
        { path },
        Date.now() - startedAt
      )
      return path
    }
  )

  ipcMain.handle("rosettacue:dialog:project", async () => {
    const startedAt = Date.now()
    recordElectronOperation(
      "select_project",
      "opened",
      "Project picker opened.",
      {}
    )
    const dialogOptions: OpenDialogOptions = {
      title: m.electron_open_project(),
      properties: ["openDirectory"],
      message: m.electron_choose_project_package(),
    }
    const result = mainWindow
      ? await dialog.showOpenDialog(mainWindow, dialogOptions)
      : await dialog.showOpenDialog(dialogOptions)
    const path = result.canceled ? null : result.filePaths[0]
    recordElectronOperation(
      "select_project",
      result.canceled ? "canceled" : "completed",
      "Project picker closed.",
      { path },
      Date.now() - startedAt
    )
    return path
  })

  ipcMain.handle("rosettacue:window:set-mode", (_event, mode: WindowMode) => {
    if (mode !== "welcome" && mode !== "workspace") {
      throw new Error(`Window mode is not allowed: ${String(mode)}`)
    }
    setWindowMode(mode)
    recordElectronOperation(
      "set_window_mode",
      "completed",
      "Main window mode changed.",
      { mode }
    )
  })

  ipcMain.handle(
    "rosettacue:diagnostics:snapshot",
    (_event, sessionId?: string) => diagnostics?.snapshot(sessionId)
  )
  ipcMain.handle(
    "rosettacue:diagnostics:set-enabled",
    async (_event, enabled: boolean) => {
      if (typeof enabled !== "boolean") {
        throw new Error("Diagnostic state must be a boolean.")
      }
      if (enabled) {
        diagnostics?.setEnabled(true)
        await backend.invoke("configure_diagnostics", { enabled: true })
      } else {
        await backend.invoke("configure_diagnostics", { enabled: false })
        diagnostics?.setEnabled(false)
      }
      return diagnostics?.snapshot()
    }
  )
  ipcMain.handle("rosettacue:diagnostics:clear", () => {
    diagnostics?.clear()
  })
  ipcMain.handle("rosettacue:diagnostics:open-window", () => {
    createDebugWindow()
  })
  ipcMain.handle(
    "rosettacue:diagnostics:renderer-error",
    (_event, message: string, details?: unknown) => {
      diagnostics?.record({
        level: "error",
        source: "renderer",
        category: "runtime",
        operation: "unhandled_error",
        phase: "reported",
        message,
        correlation_id: null,
        duration_ms: null,
        details: details ?? {},
      })
    }
  )
  ipcMain.handle(
    "rosettacue:diagnostics:export",
    async (_event, sessionId?: string) => {
      if (!diagnostics) return null
      const options = {
        title: "Export Debug Log",
        defaultPath: `rosettacue-debug-${new Date().toISOString().slice(0, 10)}.jsonl`,
        filters: [{ name: "JSON Lines", extensions: ["jsonl"] }],
      }
      const parent = debugWindow ?? mainWindow
      const result = parent
        ? await dialog.showSaveDialog(parent, options)
        : await dialog.showSaveDialog(options)
      if (result.canceled || !result.filePath) return null
      diagnostics.exportSession(result.filePath, sessionId)
      return result.filePath
    }
  )
}

app.whenReady().then(async () => {
  diagnostics = new DiagnosticStore(app.getPath("userData"))
  registerIpc()
  diagnostics.on("entry", (entry) => {
    for (const window of BrowserWindow.getAllWindows()) {
      window.webContents.send("rosettacue:diagnostics:entry", entry)
    }
  })
  diagnostics.on("enabled-changed", (enabled) => {
    for (const window of BrowserWindow.getAllWindows()) {
      window.webContents.send("rosettacue:diagnostics:enabled", enabled)
    }
  })
  diagnostics.on("cleared", () => {
    for (const window of BrowserWindow.getAllWindows()) {
      window.webContents.send("rosettacue:diagnostics:cleared")
    }
  })
  backend.on("backend-event", (event: BackendEvent, payload: unknown) => {
    if (event === "diagnostic-log") {
      diagnostics?.accept(payload)
    } else if (allowedEvents.has(event)) {
      diagnostics?.record({
        level: "debug",
        source: "backend",
        category: "event",
        operation: event,
        phase: "received",
        message: `Received backend event ${event}.`,
        correlation_id: null,
        duration_ms: null,
        details: { payload },
      })
      mainWindow?.webContents.send(`rosettacue:event:${event}`, payload)
    }
  })
  try {
    await backend.start()
  } catch (error) {
    console.error("Unable to start the Rust backend.", error)
  }
  createWindow()

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit()
  }
})

app.on("before-quit", () => {
  backend.stop()
  diagnostics?.close()
})
