import * as m from "@/paraglide/messages.js"

export function projectOpenError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  if (
    /directory is not a RosettaCue project|no such file|not found|ENOENT/i.test(
      message
    )
  ) {
    return m.project_not_found()
  }
  return message.replace(/^Error invoking remote method '[^']+': Error: /, "")
}
