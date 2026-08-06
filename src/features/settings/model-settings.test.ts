import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
  defaultWorkspaceSettings,
  loadWorkspaceSettings,
  saveWorkspaceSettings,
} from "@/features/settings/model-settings"

describe("workspace model settings persistence", () => {
  let storage: Map<string, string>

  beforeEach(() => {
    storage = new Map()
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
      clear: () => storage.clear(),
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it("restores the selected model for every task while redacting API keys", () => {
    const settings = structuredClone(defaultWorkspaceSettings)
    settings.profiles.ocr.model = "gemma-4-31b-it"
    settings.profiles.validation.model = "validator-local"
    settings.profiles.translation.model = "claude-opus-4-6"
    settings.profiles.translation.api_key = "session-secret"
    settings.debug_logging = true

    saveWorkspaceSettings(settings)
    const restored = loadWorkspaceSettings()

    expect(restored.profiles.ocr.model).toBe("gemma-4-31b-it")
    expect(restored.profiles.validation.model).toBe("validator-local")
    expect(restored.profiles.translation.model).toBe("claude-opus-4-6")
    expect(restored.profiles.translation.api_key).toBeNull()
    expect(restored.debug_logging).toBe(true)
    expect([...storage.values()].join("\n")).not.toContain("session-secret")
  })
})
