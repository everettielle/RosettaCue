import * as React from "react"
import {
  ActivityIcon,
  CheckCircle2Icon,
  FilmIcon,
  MonitorIcon,
  MonitorCogIcon,
  MoonIcon,
  RefreshCwIcon,
  SunIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Spinner } from "@/components/ui/spinner"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { useTheme } from "@/components/theme-provider"
import type {
  LlmModel,
  LlmProvider,
  MediaToolDiagnostic,
  ProviderDiagnostic,
} from "@/features/projects/types"
import {
  providerDefaults,
  type ModelTask,
  type WorkspaceSettings,
} from "@/features/settings/model-settings"
import { desktop } from "@/lib/desktop"
import { cn } from "@/lib/utils"

type Section = "general" | "project" | "models"
type Appearance = "system" | "light" | "dark"

const sections: Array<{
  value: Section
  label: string
  icon: typeof MonitorCogIcon
}> = [
  { value: "general", label: "General", icon: MonitorCogIcon },
  { value: "project", label: "Project", icon: FilmIcon },
  { value: "models", label: "Models", icon: ActivityIcon },
]

const providers: Array<{ value: LlmProvider; label: string }> = [
  { value: "lm_studio", label: "LM Studio" },
  { value: "ollama", label: "Ollama" },
  { value: "open_ai", label: "OpenAI API" },
  { value: "anthropic", label: "Anthropic API" },
]

const tasks: Array<{ value: ModelTask; label: string }> = [
  { value: "ocr", label: "OCR" },
  { value: "validation", label: "Validation" },
  { value: "translation", label: "Translation" },
]

const languageItems = [
  { value: "jpn", label: "日本語 (jpn)" },
  { value: "kor", label: "한국어 (kor)" },
  { value: "eng", label: "English (eng)" },
  { value: "zho", label: "中文 (zho)" },
]

const appearanceItems: Array<{
  value: Appearance
  label: string
  icon: typeof MonitorIcon
}> = [
  { value: "system", label: "System", icon: MonitorIcon },
  { value: "light", label: "Light", icon: SunIcon },
  { value: "dark", label: "Dark", icon: MoonIcon },
]

export function SettingsDialog({
  open,
  settings,
  onOpenChange,
  onSave,
}: {
  open: boolean
  settings: WorkspaceSettings
  onOpenChange: (open: boolean) => void
  onSave: (settings: WorkspaceSettings) => void
}) {
  const { theme, setTheme } = useTheme()
  const [appearance, setAppearance] = React.useState<Appearance>(theme)
  const [draft, setDraft] = React.useState(() => structuredClone(settings))
  const [section, setSection] = React.useState<Section>("general")
  const [activeTask, setActiveTask] = React.useState<ModelTask>("ocr")
  const [models, setModels] = React.useState<
    Partial<Record<ModelTask, LlmModel[]>>
  >({})
  const [diagnostic, setDiagnostic] = React.useState<ProviderDiagnostic | null>(
    null
  )
  const [mediaTools, setMediaTools] = React.useState<MediaToolDiagnostic[]>([])
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!open || section !== "general") {
      return
    }
    let active = true
    desktop
      .invoke<MediaToolDiagnostic[]>("media_tool_diagnostics")
      .then((tools) => {
        if (active) setMediaTools(tools)
      })
      .catch((reason: unknown) => {
        if (active)
          setError(reason instanceof Error ? reason.message : String(reason))
      })
    return () => {
      active = false
    }
  }, [open, section])

  const profile = draft.profiles[activeTask]
  const modelItems = Array.from(
    new Map(
      [
        ...(profile.model
          ? [{ value: profile.model, label: profile.model }]
          : []),
        ...(models[activeTask] ?? []).map((model) => ({
          value: model.id,
          label: model.id,
        })),
      ].map((item) => [item.value, item])
    ).values()
  )

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setDraft(structuredClone(settings))
      setAppearance(theme)
      setDiagnostic(null)
      setError(null)
    }
    onOpenChange(nextOpen)
  }

  const updateProfile = (patch: Partial<typeof profile>) => {
    setDraft((current) => ({
      ...current,
      profiles: {
        ...current.profiles,
        [activeTask]: { ...current.profiles[activeTask], ...patch },
      },
    }))
    setDiagnostic(null)
  }

  const changeProvider = (provider: LlmProvider) => {
    const defaults = providerDefaults(provider)
    updateProfile({
      provider,
      base_url: defaults.base_url,
      model: "",
      api_key: null,
    })
    setModels((current) => ({ ...current, [activeTask]: [] }))
  }

  const refreshModels = async (diagnose: boolean) => {
    setBusy(true)
    setError(null)
    setDiagnostic(null)
    try {
      if (diagnose) {
        const result = await desktop.invoke<ProviderDiagnostic>(
          "diagnose_provider",
          {
            provider: profile.provider,
            baseUrl: profile.base_url,
            apiKey: profile.api_key,
          }
        )
        setDiagnostic(result)
        setModels((current) => ({
          ...current,
          [activeTask]: result.models,
        }))
        if (!result.reachable) setError(result.message)
        return
      }
      const result = await desktop.invoke<LlmModel[]>("provider_models", {
        provider: profile.provider,
        baseUrl: profile.base_url,
        apiKey: profile.api_key,
      })
      setModels((current) => ({ ...current, [activeTask]: result }))
      if (!profile.model && result[0]) updateProfile({ model: result[0].id })
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="flex h-[min(720px,calc(100vh-3rem))] max-h-[calc(100vh-3rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl">
        <DialogHeader className="border-b px-6 pt-6 pb-4">
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure application preferences, project languages, and task
            models.
          </DialogDescription>
        </DialogHeader>

        <div className="grid min-h-0 flex-1 grid-cols-[180px_minmax(0,1fr)]">
          <nav className="flex flex-col gap-1 border-r bg-muted/30 p-3">
            {sections.map((item) => {
              const Icon = item.icon
              return (
                <Button
                  key={item.value}
                  variant={section === item.value ? "secondary" : "ghost"}
                  className="justify-start"
                  onClick={() => setSection(item.value)}
                >
                  <Icon data-icon="inline-start" />
                  {item.label}
                </Button>
              )
            })}
          </nav>

          <div className="min-w-0 overflow-y-auto p-6">
            {section === "general" && (
              <div className="flex flex-col gap-6">
                <div>
                  <h3 className="font-medium">Application</h3>
                  <p className="text-sm text-muted-foreground">
                    Appearance follows the shadcn theme tokens across every
                    window.
                  </p>
                </div>
                <Field>
                  <FieldLabel>Appearance</FieldLabel>
                  <ToggleGroup
                    aria-label="Application appearance"
                    value={[appearance]}
                    variant="outline"
                    spacing={0}
                    className="grid w-full grid-cols-3"
                    onValueChange={(values) => {
                      const next = values[0] as Appearance | undefined
                      if (next) setAppearance(next)
                    }}
                  >
                    {appearanceItems.map((item) => {
                      const Icon = item.icon
                      return (
                        <ToggleGroupItem
                          key={item.value}
                          value={item.value}
                          className="w-full"
                        >
                          <Icon data-icon="inline-start" />
                          {item.label}
                        </ToggleGroupItem>
                      )
                    })}
                  </ToggleGroup>
                </Field>
                <Separator />
                <div>
                  <h3 className="font-medium">Media tools</h3>
                  <p className="text-sm text-muted-foreground">
                    Blu-ray analysis and PGS extraction use bundled or system
                    tools.
                  </p>
                </div>
                <div className="grid gap-2">
                  {mediaTools.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      Checking media tools…
                    </p>
                  ) : (
                    mediaTools.map((tool) => (
                      <div
                        key={tool.name}
                        className="flex items-center gap-3 rounded-xl border p-3"
                      >
                        <CheckCircle2Icon
                          className={cn(
                            "size-4",
                            tool.available
                              ? "text-emerald-600"
                              : "text-destructive"
                          )}
                        />
                        <div className="min-w-0 flex-1">
                          <p className="text-sm font-medium">{tool.name}</p>
                          <p className="truncate text-xs text-muted-foreground">
                            {tool.path ?? tool.message}
                          </p>
                        </div>
                        <Badge variant="secondary">
                          {tool.available ? tool.origin : "missing"}
                        </Badge>
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}

            {section === "project" && (
              <div className="flex flex-col gap-6">
                <div>
                  <h3 className="font-medium">Project languages</h3>
                  <p className="text-sm text-muted-foreground">
                    These values are used as defaults for OCR and translation
                    jobs.
                  </p>
                </div>
                <FieldGroup>
                  <LanguageField
                    label="OCR language"
                    value={draft.ocr_language}
                    onChange={(ocr_language) =>
                      setDraft((current) => ({ ...current, ocr_language }))
                    }
                  />
                  <LanguageField
                    label="Translation target"
                    value={draft.target_language}
                    onChange={(target_language) =>
                      setDraft((current) => ({
                        ...current,
                        target_language,
                      }))
                    }
                  />
                </FieldGroup>
              </div>
            )}

            {section === "models" && (
              <div className="flex flex-col gap-5">
                <div>
                  <h3 className="font-medium">Task models</h3>
                  <p className="text-sm text-muted-foreground">
                    OCR, validation, and translation can use different
                    providers. API keys stay in memory for this app session.
                  </p>
                </div>
                <Tabs
                  value={activeTask}
                  onValueChange={(value) => {
                    setActiveTask(value as ModelTask)
                    setDiagnostic(null)
                    setError(null)
                  }}
                >
                  <TabsList className="w-full">
                    {tasks.map((task) => (
                      <TabsTrigger
                        key={task.value}
                        value={task.value}
                        className="flex-1"
                      >
                        {task.label}
                        {draft.profiles[task.value].model && (
                          <span className="size-1.5 rounded-full bg-emerald-500" />
                        )}
                      </TabsTrigger>
                    ))}
                  </TabsList>
                </Tabs>
                <FieldGroup>
                  <Field>
                    <FieldLabel>Provider</FieldLabel>
                    <Select
                      items={providers}
                      value={profile.provider}
                      onValueChange={(value) =>
                        changeProvider(value as LlmProvider)
                      }
                    >
                      <SelectTrigger className="w-full">
                        <SelectValue placeholder="Choose a provider" />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectGroup>
                          {providers.map((provider) => (
                            <SelectItem
                              key={provider.value}
                              value={provider.value}
                            >
                              {provider.label}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`base-url-${activeTask}`}>
                      Base URL
                    </FieldLabel>
                    <Input
                      id={`base-url-${activeTask}`}
                      value={profile.base_url}
                      onChange={(event) =>
                        updateProfile({ base_url: event.target.value })
                      }
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`api-key-${activeTask}`}>
                      API key
                    </FieldLabel>
                    <Input
                      id={`api-key-${activeTask}`}
                      type="password"
                      autoComplete="off"
                      value={profile.api_key ?? ""}
                      placeholder={
                        profile.provider === "lm_studio" ||
                        profile.provider === "ollama"
                          ? "Optional"
                          : "Required for this session"
                      }
                      onChange={(event) =>
                        updateProfile({ api_key: event.target.value || null })
                      }
                    />
                    <FieldDescription>
                      Keys are not written to local preferences or project
                      files.
                    </FieldDescription>
                  </Field>
                  <Field>
                    <div className="flex items-center gap-2">
                      <FieldLabel htmlFor={`model-${activeTask}`}>
                        Model
                      </FieldLabel>
                      <Button
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="ml-auto"
                        disabled={busy}
                        onClick={() => void refreshModels(false)}
                      >
                        {busy ? <Spinner /> : <RefreshCwIcon />}
                        Refresh
                      </Button>
                    </div>
                    <Select
                      items={modelItems}
                      value={profile.model || null}
                      onValueChange={(value) =>
                        updateProfile({ model: String(value ?? "") })
                      }
                    >
                      <SelectTrigger
                        id={`model-${activeTask}`}
                        className="w-full"
                        disabled={modelItems.length === 0}
                      >
                        <SelectValue placeholder="Refresh models to choose" />
                      </SelectTrigger>
                      <SelectContent alignItemWithTrigger={false}>
                        <SelectGroup>
                          {modelItems.map((model) => (
                            <SelectItem key={model.value} value={model.value}>
                              {model.label}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                    <FieldDescription>
                      Refresh to load models from this provider. A saved model
                      remains selectable while the provider is offline.
                    </FieldDescription>
                  </Field>
                </FieldGroup>
                <div className="flex items-center gap-3">
                  <Button
                    type="button"
                    variant="outline"
                    disabled={busy}
                    onClick={() => void refreshModels(true)}
                  >
                    {busy ? <Spinner /> : <ActivityIcon />}
                    Test connection
                  </Button>
                  {diagnostic?.reachable && (
                    <p className="text-sm text-emerald-600">
                      Connected in {diagnostic.latency_ms} ms ·{" "}
                      {diagnostic.models.length} models
                    </p>
                  )}
                </div>
              </div>
            )}

            {error && (
              <Alert variant="destructive" className="mt-5">
                <AlertTitle>Settings check failed</AlertTitle>
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            )}
          </div>
        </div>

        <DialogFooter className="border-t px-6 py-4">
          <Button variant="outline" onClick={() => handleOpenChange(false)}>
            Cancel
          </Button>
          <Button
            onClick={() => {
              setTheme(appearance)
              onSave(draft)
              onOpenChange(false)
            }}
          >
            Save settings
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function LanguageField({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <Field>
      <FieldLabel>{label}</FieldLabel>
      <Select
        items={languageItems}
        value={value}
        onValueChange={(next) => onChange(String(next))}
      >
        <SelectTrigger className="w-full">
          <SelectValue placeholder="Choose a language" />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {languageItems.map((language) => (
              <SelectItem key={language.value} value={language.value}>
                {language.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </Field>
  )
}
