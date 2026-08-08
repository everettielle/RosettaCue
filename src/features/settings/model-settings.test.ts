import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
  defaultLayoutTuning,
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
    settings.separate_ruby_recognition = true
    settings.profiles.ocr.model = "gemma-4-31b-it"
    settings.profiles.ruby.model = "ruby-specialist-local"
    settings.profiles.ruby.api_key = "ruby-session-secret"
    settings.profiles.validation.model = "validator-local"
    settings.profiles.translation.model = "claude-opus-4-6"
    settings.profiles.translation.api_key = "session-secret"

    saveWorkspaceSettings(settings)
    const restored = loadWorkspaceSettings()
    const persisted = [...storage.values()].join("\n")

    expect(restored.profiles.ocr.model).toBe("gemma-4-31b-it")
    expect(restored.separate_ruby_recognition).toBe(true)
    expect(restored.profiles.ruby.model).toBe("ruby-specialist-local")
    expect(restored.profiles.ruby.api_key).toBeNull()
    expect(restored.profiles.validation.model).toBe("validator-local")
    expect(restored.profiles.translation.model).toBe("claude-opus-4-6")
    expect(restored.profiles.translation.api_key).toBeNull()
    expect(persisted).not.toContain("session-secret")
    expect(persisted).not.toContain("ocr_language")
    expect(persisted).not.toContain("target_language")
  })

  it("uses combined recognition when a saved setting predates the ruby profile", () => {
    localStorage.setItem(
      "rosettacue.workspace-settings.v3",
      JSON.stringify({
        profiles: {
          ocr: { ...defaultWorkspaceSettings.profiles.ocr, model: "ocr-local" },
          validation: defaultWorkspaceSettings.profiles.validation,
          translation: defaultWorkspaceSettings.profiles.translation,
        },
      })
    )

    const restored = loadWorkspaceSettings()

    expect(restored.separate_ruby_recognition).toBe(false)
    expect(restored.profiles.ruby).toEqual(
      defaultWorkspaceSettings.profiles.ruby
    )
  })

  it("resolves a stored OpenAI profile without provider options to none", () => {
    localStorage.setItem(
      "rosettacue.workspace-settings.v3",
      JSON.stringify({
        profiles: {
          ...defaultWorkspaceSettings.profiles,
          ocr: {
            ...defaultWorkspaceSettings.profiles.ocr,
            provider: "open_ai",
            model: "gpt-5.6-luna",
          },
        },
      })
    )

    const ocr = loadWorkspaceSettings().profiles.ocr

    expect(ocr.provider).toBe("open_ai")
    expect(
      ocr.provider === "open_ai" && ocr.provider_options.reasoning_effort
    ).toBe("none")
  })

  it("restores block-detection thresholds and defaults the ones never stored", () => {
    const settings = structuredClone(defaultWorkspaceSettings)
    settings.layout.separation_em = 3.5

    saveWorkspaceSettings(settings)

    expect(loadWorkspaceSettings().layout).toEqual({
      ...defaultLayoutTuning,
      separation_em: 3.5,
    })
  })

  it("pulls stored block-detection thresholds back into their supported range", () => {
    localStorage.setItem(
      "rosettacue.workspace-settings.v3",
      JSON.stringify({
        profiles: defaultWorkspaceSettings.profiles,
        layout: {
          separation_em: 0,
          minimum_block_em2: "half an em",
          maximum_blocks: 4000,
        },
      })
    )

    // A separation of zero would cut at every blank column between glyphs, so
    // a stored value the analyzer would refuse is resolved here as well.
    expect(loadWorkspaceSettings().layout).toEqual({
      separation_em: 1.5,
      minimum_block_em2: defaultLayoutTuning.minimum_block_em2,
      maximum_blocks: 32,
    })
  })

  it("falls back to none for a reasoning effort this version no longer accepts", () => {
    localStorage.setItem(
      "rosettacue.workspace-settings.v3",
      JSON.stringify({
        profiles: {
          ...defaultWorkspaceSettings.profiles,
          ocr: {
            ...defaultWorkspaceSettings.profiles.ocr,
            provider: "open_ai",
            model: "gpt-5.6-luna",
            provider_options: { reasoning_effort: "minimal" },
          },
        },
      })
    )

    const ocr = loadWorkspaceSettings().profiles.ocr

    expect(
      ocr.provider === "open_ai" && ocr.provider_options.reasoning_effort
    ).toBe("none")
  })

  it("drops provider options stored on a provider that takes none", () => {
    localStorage.setItem(
      "rosettacue.workspace-settings.v3",
      JSON.stringify({
        profiles: {
          ...defaultWorkspaceSettings.profiles,
          ocr: {
            ...defaultWorkspaceSettings.profiles.ocr,
            provider_options: { reasoning_effort: "high" },
          },
        },
      })
    )

    const ocr = loadWorkspaceSettings().profiles.ocr

    expect(ocr.provider).toBe("lm_studio")
    expect("provider_options" in ocr).toBe(false)
  })
})
