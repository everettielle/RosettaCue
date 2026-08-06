import type { LlmProvider, ProviderConfig } from "@/features/projects/types"
import * as m from "@/paraglide/messages.js"

const STORAGE_KEY = "rosettacue.workspace-settings.v1"

export type ModelTask = "ocr" | "validation" | "translation"

export type WorkspaceSettings = {
  ocr_language: string
  target_language: string
  debug_logging: boolean
  profiles: Record<ModelTask, ProviderConfig>
}

export function providerDefaults(provider: LlmProvider): ProviderConfig {
  const baseUrls: Record<LlmProvider, string> = {
    lm_studio: "http://127.0.0.1:1234/v1",
    ollama: "http://127.0.0.1:11434/v1",
    open_ai: "https://api.openai.com/v1",
    anthropic: "https://api.anthropic.com/v1",
  }

  return {
    provider,
    base_url: baseUrls[provider],
    model: "",
    api_key: null,
    timeout_seconds: 120,
    max_tokens: 512,
    max_attempts: 2,
  }
}

export const defaultWorkspaceSettings: WorkspaceSettings = {
  ocr_language: "jpn",
  target_language: "kor",
  debug_logging: false,
  profiles: {
    ocr: providerDefaults("lm_studio"),
    validation: providerDefaults("lm_studio"),
    translation: {
      ...providerDefaults("lm_studio"),
      max_tokens: 1024,
    },
  },
}

function mergeProfile(
  value: Partial<ProviderConfig> | undefined,
  fallback: ProviderConfig
): ProviderConfig {
  const provider = value?.provider ?? fallback.provider
  return {
    ...providerDefaults(provider),
    ...fallback,
    ...value,
    provider,
    api_key: null,
  }
}

export function loadWorkspaceSettings(): WorkspaceSettings {
  try {
    const stored = JSON.parse(
      localStorage.getItem(STORAGE_KEY) ?? "null"
    ) as Partial<WorkspaceSettings> | null
    if (stored?.profiles) {
      return {
        ocr_language:
          stored.ocr_language ?? defaultWorkspaceSettings.ocr_language,
        target_language:
          stored.target_language ?? defaultWorkspaceSettings.target_language,
        debug_logging:
          stored.debug_logging ?? defaultWorkspaceSettings.debug_logging,
        profiles: {
          ocr: mergeProfile(
            stored.profiles.ocr,
            defaultWorkspaceSettings.profiles.ocr
          ),
          validation: mergeProfile(
            stored.profiles.validation,
            defaultWorkspaceSettings.profiles.validation
          ),
          translation: mergeProfile(
            stored.profiles.translation,
            defaultWorkspaceSettings.profiles.translation
          ),
        },
      }
    }
  } catch {
    // Invalid settings are replaced by deterministic defaults.
  }
  return structuredClone(defaultWorkspaceSettings)
}

export function saveWorkspaceSettings(settings: WorkspaceSettings) {
  const redacted: WorkspaceSettings = {
    ...settings,
    profiles: {
      ocr: { ...settings.profiles.ocr, api_key: null },
      validation: { ...settings.profiles.validation, api_key: null },
      translation: { ...settings.profiles.translation, api_key: null },
    },
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(redacted))
}

export function validateProviderConfig(config: ProviderConfig) {
  if (!config.base_url.trim()) {
    return m.provider_base_url_required()
  }
  if (!config.model.trim()) {
    return m.provider_model_required()
  }
  if (
    (config.provider === "open_ai" || config.provider === "anthropic") &&
    !config.api_key?.trim()
  ) {
    return m.provider_api_key_required({
      provider: config.provider === "open_ai" ? "OpenAI" : "Anthropic",
    })
  }
  return null
}
