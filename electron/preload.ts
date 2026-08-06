import { contextBridge, ipcRenderer } from "electron"

import {
  backendEvents,
  backendMethods,
  windowModes,
  type BackendEvent,
  type BackendMethod,
  type WindowMode,
} from "./contracts"

const allowedMethods = new Set<string>(backendMethods)
const allowedEvents = new Set<string>(backendEvents)
const allowedWindowModes = new Set<string>(windowModes)

contextBridge.exposeInMainWorld("rosettaCue", {
  platform: process.platform,
  invoke(method: BackendMethod, params?: unknown) {
    if (!allowedMethods.has(method)) {
      return Promise.reject(
        new Error(`Backend method is not allowed: ${method}`)
      )
    }
    return ipcRenderer.invoke("rosettacue:backend:invoke", method, params ?? {})
  },
  on(event: BackendEvent, listener: (payload: unknown) => void) {
    if (!allowedEvents.has(event)) {
      throw new Error(`Backend event is not allowed: ${event}`)
    }
    const channel = `rosettacue:event:${event}`
    const wrapped = (_ipcEvent: Electron.IpcRendererEvent, payload: unknown) =>
      listener(payload)
    ipcRenderer.on(channel, wrapped)
    return () => ipcRenderer.removeListener(channel, wrapped)
  },
  dialogs: {
    selectDirectory(options?: { title?: string; defaultPath?: string }) {
      return ipcRenderer.invoke("rosettacue:dialog:directory", options)
    },
    selectProject() {
      return ipcRenderer.invoke("rosettacue:dialog:project")
    },
  },
  window: {
    setMode(mode: WindowMode) {
      if (!allowedWindowModes.has(mode)) {
        return Promise.reject(new Error(`Window mode is not allowed: ${mode}`))
      }
      return ipcRenderer.invoke("rosettacue:window:set-mode", mode)
    },
  },
})
