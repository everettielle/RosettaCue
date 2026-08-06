import * as m from "@/paraglide/messages.js"

const INVALID_PROJECT_CHARACTER = /[\\/:*?"<>|]/u

function containsControlCharacter(value: string) {
  return [...value].some((character) => character.charCodeAt(0) < 32)
}

export function projectNameError(value: string) {
  const name = value.trim()
  if (!name) {
    return m.project_name_required()
  }
  if (INVALID_PROJECT_CHARACTER.test(name) || containsControlCharacter(name)) {
    return m.project_name_invalid()
  }
  return null
}
