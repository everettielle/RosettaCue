import { describe, expect, it } from "vitest"

import { projectNameError } from "@/lib/project-name"

describe("projectNameError", () => {
  it("accepts a normal film title", () => {
    expect(projectNameError("Belle - Disc 1")).toBeNull()
  })

  it("rejects empty names and path separators", () => {
    expect(projectNameError("  ")).not.toBeNull()
    expect(projectNameError("Disc/1")).not.toBeNull()
    expect(projectNameError("Disc\\1")).not.toBeNull()
  })
})
