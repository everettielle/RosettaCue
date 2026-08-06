import { describe, expect, it } from "vitest"

import { projectOpenError } from "@/features/projects/project-errors"

describe("projectOpenError", () => {
  it("maps a deleted project package to a recoverable message", () => {
    expect(
      projectOpenError(
        new Error(
          "Error invoking remote method 'rosettacue:backend:invoke': Error: directory is not a RosettaCue project: /tmp/Belle.rosettacue"
        )
      )
    ).toBe(
      "Project not found. It may have been moved or deleted. Remove it from Recent Projects or choose another project."
    )
  })

  it("removes Electron's remote-method wrapper from other project errors", () => {
    expect(
      projectOpenError(
        new Error(
          "Error invoking remote method 'rosettacue:backend:invoke': Error: project is locked"
        )
      )
    ).toBe("project is locked")
  })
})
