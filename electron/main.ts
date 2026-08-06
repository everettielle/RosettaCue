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
import { RustBackend } from "./rust-backend"
import * as m from "../src/paraglide/messages.js"

const currentDirectory = dirname(fileURLToPath(import.meta.url))
const appRoot = join(currentDirectory, "..")
const rendererDirectory = join(appRoot, "dist")
const allowedMethods = new Set<string>(backendMethods)
const allowedEvents = new Set<string>(backendEvents)

let mainWindow: BrowserWindow | null = null
const backend = new RustBackend()

const welcomeWindowSize = { width: 960, height: 600 }
const workspaceWindowSize = { width: 1440, height: 900 }

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
      const dialogOptions: OpenDialogOptions = {
        title: options?.title,
        defaultPath: options?.defaultPath,
        properties: ["openDirectory", "createDirectory"],
      }
      const result = mainWindow
        ? await dialog.showOpenDialog(mainWindow, dialogOptions)
        : await dialog.showOpenDialog(dialogOptions)
      return result.canceled ? null : result.filePaths[0]
    }
  )

  ipcMain.handle("rosettacue:dialog:project", async () => {
    const dialogOptions: OpenDialogOptions = {
      title: m.electron_open_project(),
      properties: ["openDirectory"],
      message: m.electron_choose_project_package(),
    }
    const result = mainWindow
      ? await dialog.showOpenDialog(mainWindow, dialogOptions)
      : await dialog.showOpenDialog(dialogOptions)
    return result.canceled ? null : result.filePaths[0]
  })

  ipcMain.handle("rosettacue:window:set-mode", (_event, mode: WindowMode) => {
    if (mode !== "welcome" && mode !== "workspace") {
      throw new Error(`Window mode is not allowed: ${String(mode)}`)
    }
    setWindowMode(mode)
  })
}

app.whenReady().then(async () => {
  registerIpc()
  backend.on("backend-event", (event: BackendEvent, payload: unknown) => {
    if (allowedEvents.has(event)) {
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

app.on("before-quit", () => backend.stop())
