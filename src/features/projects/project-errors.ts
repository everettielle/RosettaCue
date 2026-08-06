export function projectOpenError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  if (
    /directory is not a RosettaCue project|no such file|not found|ENOENT/i.test(
      message
    )
  ) {
    return "Project not found. It may have been moved or deleted. Remove it from Recent Projects or choose another project."
  }
  return message.replace(/^Error invoking remote method '[^']+': Error: /, "")
}
