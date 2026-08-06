import * as React from "react"
import {
  ActivityIcon,
  BugIcon,
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
  FieldContent,
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
import { Switch } from "@/components/ui/switch"
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
import * as m from "@/paraglide/messages.js"

type Section = "general" | "project" | "models" | "advanced"
type Appearance = "system" | "light" | "dark"

const sections: Array<{
  value: Section
  label: string
  icon: typeof MonitorCogIcon
}> = [
  { value: "general", label: m.settings_general(), icon: MonitorCogIcon },
  { value: "project", label: m.settings_project(), icon: FilmIcon },
  { value: "models", label: m.settings_models(), icon: ActivityIcon },
  { value: "advanced", label: m.settings_advanced(), icon: BugIcon },
]

const providers: Array<{ value: LlmProvider; label: string }> = [
  { value: "lm_studio", label: "LM Studio" },
  { value: "ollama", label: "Ollama" },
  { value: "open_ai", label: "OpenAI API" },
  { value: "anthropic", label: "Anthropic API" },
]

const tasks: Array<{ value: ModelTask; label: string }> = [
  { value: "ocr", label: m.settings_task_ocr() },
  { value: "validation", label: m.settings_task_validation() },
  { value: "translation", label: m.settings_task_translation() },
]

function taskHelp(task: ModelTask) {
  const help: Record<
    ModelTask,
    { phase: () => string; description: () => string }
  > = {
    ocr: {
      phase: m.settings_phase_1,
      description: m.settings_task_ocr_description,
    },
    validation: {
      phase: m.settings_phase_2,
      description: m.settings_task_validation_description,
    },
    translation: {
      phase: m.settings_post_ocr,
      description: m.settings_task_translation_description,
    },
  }
  return help[task]
}

const languageItems = [
  { value: "zho", label: m.language_chinese() },
  { value: "eng", label: m.language_english() },
  { value: "fra", label: m.language_french() },
  { value: "deu", label: m.language_german() },
  { value: "ita", label: m.language_italian() },
  { value: "jpn", label: m.language_japanese() },
  { value: "kor", label: m.language_korean() },
  { value: "spa", label: m.language_spanish() },
]

const appearanceItems: Array<{
  value: Appearance
  label: string
  icon: typeof MonitorIcon
}> = [
  { value: "system", label: m.settings_theme_system(), icon: MonitorIcon },
  { value: "light", label: m.settings_theme_light(), icon: SunIcon },
  { value: "dark", label: m.settings_theme_dark(), icon: MoonIcon },
]

function mediaToolOriginLabel(origin: MediaToolDiagnostic["origin"]) {
  const labels: Record<
    NonNullable<MediaToolDiagnostic["origin"]>,
    () => string
  > = {
    configured: m.settings_media_tools_origin_configured,
    bundled: m.settings_media_tools_origin_bundled,
    path: m.settings_media_tools_origin_path,
    system: m.settings_media_tools_origin_system,
  }
  return origin ? labels[origin]() : m.common_missing()
}

export function SettingsDialog({
  open,
  settings,
  debugLogCount,
  onOpenChange,
  onOpenDebugLog,
  onSave,
}: {
  open: boolean
  settings: WorkspaceSettings
  debugLogCount: number
  onOpenChange: (open: boolean) => void
  onOpenDebugLog: () => void
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
          <DialogTitle>{m.settings_title()}</DialogTitle>
          <DialogDescription>{m.settings_description()}</DialogDescription>
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
                  <h3 className="font-medium">{m.settings_application()}</h3>
                  <p className="text-sm text-muted-foreground">
                    {m.settings_appearance_description()}
                  </p>
                </div>
                <Field>
                  <FieldLabel>{m.settings_appearance()}</FieldLabel>
                  <ToggleGroup
                    aria-label={m.settings_appearance_label()}
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
                  <h3 className="font-medium">{m.settings_media_tools()}</h3>
                  <p className="text-sm text-muted-foreground">
                    {m.settings_media_tools_description()}
                  </p>
                </div>
                <div className="grid gap-2">
                  {mediaTools.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      {m.settings_media_tools_checking()}
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
                          {tool.available
                            ? mediaToolOriginLabel(tool.origin)
                            : m.common_missing()}
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
                  <h3 className="font-medium">
                    {m.settings_project_languages()}
                  </h3>
                  <p className="text-sm text-muted-foreground">
                    {m.settings_project_languages_description()}
                  </p>
                </div>
                <FieldGroup>
                  <LanguageField
                    label={m.settings_ocr_language()}
                    value={draft.ocr_language}
                    onChange={(ocr_language) =>
                      setDraft((current) => ({ ...current, ocr_language }))
                    }
                  />
                  <LanguageField
                    label={m.settings_translation_target()}
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
                  <h3 className="font-medium">{m.settings_task_models()}</h3>
                  <p className="text-sm text-muted-foreground">
                    {m.settings_task_models_description()}
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
                <Alert>
                  <AlertTitle>{taskHelp(activeTask).phase()}</AlertTitle>
                  <AlertDescription>
                    {taskHelp(activeTask).description()}
                  </AlertDescription>
                </Alert>
                <FieldGroup>
                  <Field>
                    <FieldLabel>{m.settings_provider()}</FieldLabel>
                    <Select
                      items={providers}
                      value={profile.provider}
                      onValueChange={(value) =>
                        changeProvider(value as LlmProvider)
                      }
                    >
                      <SelectTrigger className="w-full">
                        <SelectValue
                          placeholder={m.settings_choose_provider()}
                        />
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
                      {m.settings_base_url()}
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
                      {m.settings_api_key()}
                    </FieldLabel>
                    <Input
                      id={`api-key-${activeTask}`}
                      type="password"
                      autoComplete="off"
                      value={profile.api_key ?? ""}
                      placeholder={
                        profile.provider === "lm_studio" ||
                        profile.provider === "ollama"
                          ? m.common_optional()
                          : m.common_required_for_session()
                      }
                      onChange={(event) =>
                        updateProfile({ api_key: event.target.value || null })
                      }
                    />
                    <FieldDescription>
                      {m.settings_api_key_description()}
                    </FieldDescription>
                  </Field>
                  <Field>
                    <div className="flex items-center gap-2">
                      <FieldLabel htmlFor={`model-${activeTask}`}>
                        {m.settings_model()}
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
                        {m.common_refresh()}
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
                        <SelectValue
                          placeholder={m.settings_refresh_models_placeholder()}
                        />
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
                      {m.settings_model_description()}
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
                    {m.settings_test_connection()}
                  </Button>
                  {diagnostic?.reachable && (
                    <p className="text-sm text-emerald-600">
                      {m.settings_connected({
                        latency: diagnostic.latency_ms,
                        count: diagnostic.models.length,
                      })}
                    </p>
                  )}
                </div>
              </div>
            )}

            {section === "advanced" && (
              <div className="flex flex-col gap-6">
                <div>
                  <h3 className="font-medium">{m.settings_debugging()}</h3>
                  <p className="text-sm text-muted-foreground">
                    {m.settings_debugging_description()}
                  </p>
                </div>
                <Field orientation="horizontal">
                  <FieldContent>
                    <FieldLabel htmlFor="debug-logging">
                      {m.settings_debug_logging()}
                    </FieldLabel>
                    <FieldDescription>
                      {m.settings_debug_logging_description()}
                    </FieldDescription>
                  </FieldContent>
                  <Switch
                    id="debug-logging"
                    checked={draft.debug_logging}
                    onCheckedChange={(checked) =>
                      setDraft((current) => ({
                        ...current,
                        debug_logging: checked,
                      }))
                    }
                  />
                </Field>
                <Separator />
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium">
                      {m.settings_debug_log()}
                    </p>
                    <p className="text-sm text-muted-foreground">
                      {m.settings_debug_log_summary({ count: debugLogCount })}
                    </p>
                  </div>
                  <Button variant="outline" onClick={onOpenDebugLog}>
                    <BugIcon data-icon="inline-start" />
                    {m.settings_open_debug_log()}
                  </Button>
                </div>
              </div>
            )}

            {error && (
              <Alert variant="destructive" className="mt-5">
                <AlertTitle>{m.settings_check_failed()}</AlertTitle>
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            )}
          </div>
        </div>

        <DialogFooter className="border-t px-6 py-4">
          <Button variant="outline" onClick={() => handleOpenChange(false)}>
            {m.common_cancel()}
          </Button>
          <Button
            onClick={() => {
              setTheme(appearance)
              onSave(draft)
              onOpenChange(false)
            }}
          >
            {m.settings_save()}
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
          <SelectValue placeholder={m.settings_choose_language()} />
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
