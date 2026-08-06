const INVALID_PROJECT_CHARACTER = /[\\/:*?"<>|]/u

function containsControlCharacter(value: string) {
  return [...value].some((character) => character.charCodeAt(0) < 32)
}

export function projectNameError(value: string) {
  const name = value.trim()
  if (!name) {
    return "Enter a project name."
  }
  if (INVALID_PROJECT_CHARACTER.test(name) || containsControlCharacter(name)) {
    return "Project names cannot contain path separators or reserved characters."
  }
  return null
}
