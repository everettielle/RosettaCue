import * as React from "react"

import { TooltipProvider } from "@/components/ui/tooltip"
import { CreateProjectDialog } from "@/features/projects/create-project-dialog"
import {
  loadRecentProjects,
  removeRecentProject,
  rememberProject,
  type RecentProject,
} from "@/features/projects/recent-projects"
import { projectOpenError } from "@/features/projects/project-errors"
import type { BackendInfo, ProjectOverview } from "@/features/projects/types"
import { WelcomeScreen } from "@/features/welcome/welcome-screen"
import { ProjectWorkspace } from "@/features/workspace/project-workspace"
import { desktop } from "@/lib/desktop"
import { projectNameError } from "@/lib/project-name"

function App() {
  const [backendInfo, setBackendInfo] = React.useState<BackendInfo | null>(null)
  const [backendError, setBackendError] = React.useState<string | null>(null)
  const [projectError, setProjectError] = React.useState<string | null>(null)
  const [project, setProject] = React.useState<ProjectOverview | null>(null)
  const [recentProjects, setRecentProjects] = React.useState(loadRecentProjects)
  const [createOpen, setCreateOpen] = React.useState(false)
  const [projectName, setProjectName] = React.useState("")
  const [projectParent, setProjectParent] = React.useState("")
  const [createError, setCreateError] = React.useState<string | null>(null)
  const [busy, setBusy] = React.useState(false)

  React.useEffect(() => {
    let active = true
    void desktop.window.setMode("welcome")
    desktop
      .invoke<BackendInfo>("backend_info")
      .then((info) => {
        if (active) {
          setBackendInfo(info)
          setBackendError(null)
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setBackendError(
            error instanceof Error ? error.message : String(error)
          )
        }
      })
    return () => {
      active = false
    }
  }, [])

  const activateProject = async (overview: ProjectOverview) => {
    setProjectError(null)
    setProject(overview)
    setRecentProjects(rememberProject(overview))
    await desktop.window.setMode("workspace")
  }

  const openProjectAtPath = async (path: string) => {
    setProjectError(null)
    setBusy(true)
    try {
      const overview = await desktop.invoke<ProjectOverview>("open_project", {
        path,
      })
      await activateProject(overview)
      return overview
    } catch (error) {
      setProjectError(projectOpenError(error))
    } finally {
      setBusy(false)
    }
  }

  const openProject = async () => {
    setProjectError(null)
    setBusy(true)
    try {
      const path = await desktop.dialogs.selectProject()
      if (!path) {
        return
      }
      const overview = await desktop.invoke<ProjectOverview>("open_project", {
        path,
      })
      await activateProject(overview)
    } catch (error) {
      setProjectError(projectOpenError(error))
    } finally {
      setBusy(false)
    }
  }

  const chooseProjectParent = async () => {
    try {
      const path = await desktop.dialogs.selectDirectory({
        title: "Choose where to save the project",
        defaultPath: projectParent || undefined,
      })
      if (path) {
        setProjectParent(path)
      }
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : String(error))
    }
  }

  const createProject = async (event: React.FormEvent) => {
    event.preventDefault()
    const nameError = projectNameError(projectName)
    if (nameError) {
      setCreateError(nameError)
      return
    }
    if (!projectParent) {
      setCreateError("Choose a folder for the project.")
      return
    }

    setBusy(true)
    setCreateError(null)
    try {
      const overview = await desktop.invoke<ProjectOverview>("create_project", {
        parent: projectParent,
        name: projectName.trim(),
      })
      setCreateOpen(false)
      setProjectName("")
      setProjectParent("")
      await activateProject(overview)
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <TooltipProvider>
      {project ? (
        <ProjectWorkspace
          project={project}
          onProjectChange={(next) => {
            setProject(next)
            setRecentProjects(rememberProject(next))
          }}
        />
      ) : (
        <WelcomeScreen
          backendInfo={backendInfo}
          backendError={backendError}
          projectError={projectError}
          recentProjects={recentProjects}
          busy={busy}
          onCreate={() => setCreateOpen(true)}
          onOpen={() => void openProject()}
          onOpenRecent={(recent: RecentProject) =>
            openProjectAtPath(recent.path)
          }
          onRemoveRecent={(recent: RecentProject) => {
            setRecentProjects(removeRecentProject(recent.path))
            setProjectError(null)
          }}
        />
      )}

      <CreateProjectDialog
        open={createOpen}
        busy={busy}
        name={projectName}
        parent={projectParent}
        error={createError}
        onOpenChange={(open) => {
          if (!busy) {
            setCreateOpen(open)
            setCreateError(null)
          }
        }}
        onNameChange={setProjectName}
        onChooseParent={() => void chooseProjectParent()}
        onSubmit={createProject}
      />
    </TooltipProvider>
  )
}

export default App
