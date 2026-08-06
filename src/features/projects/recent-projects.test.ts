import { beforeEach, describe, expect, it, vi } from "vitest"

import {
  loadRecentProjects,
  removeRecentProject,
} from "@/features/projects/recent-projects"

class MemoryStorage {
  private values = new Map<string, string>()

  getItem(key: string) {
    return this.values.get(key) ?? null
  }

  setItem(key: string, value: string) {
    this.values.set(key, value)
  }

  clear() {
    this.values.clear()
  }
}

describe("recent projects", () => {
  const storage = new MemoryStorage()

  beforeEach(() => {
    storage.clear()
    vi.stubGlobal("localStorage", storage)
  })

  it("removes only the requested project", () => {
    localStorage.setItem(
      "rosettacue.recent-projects",
      JSON.stringify([
        {
          path: "/tmp/Belle.rosettacue",
          name: "Belle",
          updatedAt: "2026-08-06T00:00:00Z",
        },
        {
          path: "/tmp/Other.rosettacue",
          name: "Other",
          updatedAt: "2026-08-06T00:01:00Z",
        },
      ])
    )

    expect(removeRecentProject("/tmp/Belle.rosettacue")).toEqual([
      {
        path: "/tmp/Other.rosettacue",
        name: "Other",
        updatedAt: "2026-08-06T00:01:00Z",
      },
    ])
    expect(loadRecentProjects()).toHaveLength(1)
  })
})
