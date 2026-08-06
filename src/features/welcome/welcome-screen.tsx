import {
  Clock3Icon,
  FolderOpenIcon,
  FolderXIcon,
  PlusIcon,
  ServerOffIcon,
  XIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { ScrollArea } from "@/components/ui/scroll-area"
import type { BackendInfo, ProjectOverview } from "@/features/projects/types"
import type { RecentProject } from "@/features/projects/recent-projects"

function formatRecentDate(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return "Recently opened"
  }
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date)
}

export function WelcomeScreen({
  backendInfo,
  backendError,
  projectError,
  recentProjects,
  busy,
  onCreate,
  onOpen,
  onOpenRecent,
  onRemoveRecent,
}: {
  backendInfo: BackendInfo | null
  backendError: string | null
  projectError: string | null
  recentProjects: RecentProject[]
  busy: boolean
  onCreate: () => void
  onOpen: () => void
  onOpenRecent: (recent: RecentProject) => Promise<ProjectOverview | void>
  onRemoveRecent: (recent: RecentProject) => void
}) {
  return (
    <main className="relative grid h-svh grid-cols-[3fr_2fr] overflow-hidden bg-background text-foreground">
      <div className="absolute inset-x-0 top-0 h-11 [-webkit-app-region:drag]" />

      <section className="flex min-h-0 flex-col border-r bg-muted/40 px-12 py-12">
        <div className="flex flex-1 flex-col items-center justify-center gap-8">
          <div className="flex flex-col items-center gap-4 text-center">
            <img
              src="./rosettacue-mark.png"
              alt="RosettaCue"
              className="size-28 rounded-3xl shadow-lg"
            />
            <div className="flex flex-col items-center gap-1">
              <h1 className="font-heading text-3xl font-semibold tracking-tight">
                RosettaCue
              </h1>
              <p className="text-sm text-muted-foreground">
                Image subtitles, ready for review
              </p>
              <p className="text-xs text-muted-foreground">
                Version {backendInfo?.version ?? "0.1.0"}
              </p>
            </div>
          </div>

          <div className="flex w-full max-w-sm flex-col gap-3 [-webkit-app-region:no-drag]">
            <Button
              size="lg"
              variant="secondary"
              className="w-full justify-start"
              onClick={onCreate}
              disabled={busy || Boolean(backendError)}
            >
              <PlusIcon data-icon="inline-start" />
              Create a new project
            </Button>
            <Button
              size="lg"
              variant="secondary"
              className="w-full justify-start"
              onClick={onOpen}
              disabled={busy || Boolean(backendError)}
            >
              <FolderOpenIcon data-icon="inline-start" />
              Open an existing project
            </Button>
          </div>

          {backendError && (
            <Alert variant="destructive" className="max-w-sm">
              <ServerOffIcon />
              <AlertTitle>Core unavailable</AlertTitle>
              <AlertDescription>{backendError}</AlertDescription>
            </Alert>
          )}
          {projectError && (
            <Alert variant="destructive" className="max-w-sm">
              <FolderXIcon />
              <AlertTitle>Unable to open project</AlertTitle>
              <AlertDescription>{projectError}</AlertDescription>
            </Alert>
          )}
        </div>
      </section>

      <section className="flex min-h-0 flex-col px-8 pt-12 pb-8">
        {recentProjects.length === 0 ? (
          <Empty className="border-0 p-6">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Clock3Icon />
              </EmptyMedia>
              <EmptyTitle>No Recent Projects</EmptyTitle>
              <EmptyDescription>
                Projects you create or open will appear here.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col gap-4">
            <div className="flex flex-col gap-1 px-2">
              <h2 className="font-heading text-lg font-medium">
                Recent Projects
              </h2>
              <p className="text-xs text-muted-foreground">
                Continue where you left off.
              </p>
            </div>
            <ScrollArea className="min-h-0 flex-1">
              <div className="flex flex-col gap-1 pr-3 [-webkit-app-region:no-drag]">
                {recentProjects.map((recent) => (
                  <div
                    key={recent.path}
                    className="group flex items-center gap-1 rounded-2xl hover:bg-muted"
                  >
                    <Button
                      variant="ghost"
                      className="h-auto min-w-0 flex-1 justify-start px-3 py-3 text-left hover:bg-transparent"
                      onClick={() => void onOpenRecent(recent)}
                      disabled={busy}
                    >
                      <span className="flex min-w-0 flex-1 flex-col gap-1">
                        <span className="truncate font-medium">
                          {recent.name}
                        </span>
                        <span className="truncate text-xs font-normal text-muted-foreground">
                          {recent.path}
                        </span>
                        <span className="text-xs font-normal text-muted-foreground">
                          {formatRecentDate(recent.updatedAt)}
                        </span>
                      </span>
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      className="mr-2 text-muted-foreground"
                      aria-label={`Remove ${recent.name} from Recent Projects`}
                      title="Remove from Recent Projects"
                      disabled={busy}
                      onClick={() => onRemoveRecent(recent)}
                    >
                      <XIcon />
                    </Button>
                  </div>
                ))}
              </div>
            </ScrollArea>
          </div>
        )}
      </section>
    </main>
  )
}
