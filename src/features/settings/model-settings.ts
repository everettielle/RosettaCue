import type {
  LlmProvider,
  ProviderCommon,
  ProviderConfig,
  ProviderSpec,
  ReasoningEffort,
} from "@/features/projects/types"
import * as m from "@/paraglide/messages.js"

// v3: provider-specific parameters moved under provider_options; earlier
// shapes are ignored.
const STORAGE_KEY = "rosettacue.workspace-settings.v3"

export type ModelTask = "ocr" | "ruby" | "validation" | "translation"

export type WorkspaceSettings = {
  separate_ruby_recognition: boolean
  profiles: Record<ModelTask, ProviderConfig>
}

const COMMON_KEYS = [
  "base_url",
  "model",
  "api_key",
  "timeout_seconds",
  "max_tokens",
  "max_attempts",
] as const satisfies ReadonlyArray<keyof ProviderCommon>

function commonDefaults(provider: LlmProvider): ProviderCommon {
  const baseUrls: Record<LlmProvider, string> = {
    lm_studio: "http://127.0.0.1:1234/v1",
    ollama: "http://127.0.0.1:11434/v1",
    open_ai: "https://api.openai.com/v1",
    anthropic: "https://api.anthropic.com/v1",
  }

  return {
    base_url: baseUrls[provider],
    model: "",
    api_key: null,
    timeout_seconds: 120,
    max_tokens: 512,
    max_attempts: 2,
  }
}

/** The only place that resolves stored provider-specific values. */
function normalizeSpec(
  provider: LlmProvider,
  storedEffort: ReasoningEffort | null | undefined
): ProviderSpec {
  switch (provider) {
    case "open_ai":
      return {
        provider,
        provider_options: { reasoning_effort: storedEffort ?? "none" },
      }
    case "lm_studio":
    case "ollama":
    case "anthropic":
      return { provider }
  }
}

export function providerDefaults(provider: LlmProvider): ProviderConfig {
  return { ...commonDefaults(provider), ...normalizeSpec(provider, undefined) }
}

export const defaultWorkspaceSettings: WorkspaceSettings = {
  separate_ruby_recognition: false,
  profiles: {
    ocr: providerDefaults("lm_studio"),
    ruby: providerDefaults("lm_studio"),
    validation: providerDefaults("lm_studio"),
    translation: {
      ...providerDefaults("lm_studio"),
      max_tokens: 1024,
    },
  },
}

function pickCommon(
  value: Partial<ProviderCommon> | undefined
): Partial<ProviderCommon> {
  const picked: Partial<ProviderCommon> = {}
  if (!value) return picked
  const copy = <K extends keyof ProviderCommon>(key: K) => {
    if (value[key] !== undefined) picked[key] = value[key]
  }
  COMMON_KEYS.forEach(copy)
  return picked
}

function mergeProfile(
  value: Partial<ProviderConfig> | undefined,
  fallback: ProviderConfig
): ProviderConfig {
  const provider = value?.provider ?? fallback.provider
  const storedEffort =
    value && "provider_options" in value
      ? value.provider_options?.reasoning_effort
      : undefined
  // Common fields are picked by name so that stored data cannot smuggle
  // another provider's parameters past the spec normalization.
  return {
    ...commonDefaults(provider),
    ...pickCommon(fallback),
    ...pickCommon(value),
    api_key: null,
    ...normalizeSpec(provider, storedEffort),
  }
}

export function loadWorkspaceSettings(): WorkspaceSettings {
  try {
    const stored = JSON.parse(
      localStorage.getItem(STORAGE_KEY) ?? "null"
    ) as Partial<WorkspaceSettings> | null
    if (stored?.profiles) {
      return {
        separate_ruby_recognition:
          stored.separate_ruby_recognition ??
          defaultWorkspaceSettings.separate_ruby_recognition,
        profiles: {
          ocr: mergeProfile(
            stored.profiles.ocr,
            defaultWorkspaceSettings.profiles.ocr
          ),
          ruby: mergeProfile(
            stored.profiles.ruby,
            defaultWorkspaceSettings.profiles.ruby
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
      ruby: { ...settings.profiles.ruby, api_key: null },
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
