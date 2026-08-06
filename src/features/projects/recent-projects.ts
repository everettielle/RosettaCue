import type { ProjectOverview } from "@/features/projects/types"

const storageKey = "rosettacue.recent-projects"
const maximumRecentProjects = 8

export type RecentProject = {
  path: string
  name: string
  updatedAt: string
}

function isRecentProject(value: unknown): value is RecentProject {
  if (!value || typeof value !== "object") {
    return false
  }
  const candidate = value as Partial<RecentProject>
  return (
    typeof candidate.path === "string" &&
    typeof candidate.name === "string" &&
    typeof candidate.updatedAt === "string"
  )
}

export function loadRecentProjects(): RecentProject[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(storageKey) ?? "[]")
    return Array.isArray(parsed) ? parsed.filter(isRecentProject) : []
  } catch {
    return []
  }
}

export function rememberProject(project: ProjectOverview): RecentProject[] {
  const recent = {
    path: project.path,
    name: project.metadata.name,
    updatedAt: project.metadata.updated_at,
  }
  const next = [
    recent,
    ...loadRecentProjects().filter((item) => item.path !== project.path),
  ].slice(0, maximumRecentProjects)
  localStorage.setItem(storageKey, JSON.stringify(next))
  return next
}

export function removeRecentProject(path: string): RecentProject[] {
  const next = loadRecentProjects().filter((item) => item.path !== path)
  localStorage.setItem(storageKey, JSON.stringify(next))
  return next
}
