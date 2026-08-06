import type {
  BackendEvent,
  BackendMethod,
  RosettaCueDesktopApi,
} from "@/types/desktop"

function unavailable(): never {
  throw new Error("RosettaCue desktop APIs are available only inside Electron.")
}

const browserFallback: RosettaCueDesktopApi = {
  platform: "browser",
  invoke: async () => unavailable(),
  on: () => () => undefined,
  dialogs: {
    selectDirectory: async () => unavailable(),
    selectProject: async () => unavailable(),
  },
  window: {
    setMode: async () => undefined,
  },
}

const bridge = window.rosettaCue ?? browserFallback

export const desktop = {
  available: Boolean(window.rosettaCue),
  platform: bridge.platform,
  invoke<T>(method: BackendMethod, params?: unknown) {
    return bridge.invoke(method, params) as Promise<T>
  },
  on<T>(event: BackendEvent, listener: (payload: T) => void) {
    return bridge.on(event, (payload) => listener(payload as T))
  },
  dialogs: bridge.dialogs,
  window: bridge.window,
}
