import { safeStorage } from "electron"
import { existsSync, readFileSync, unlinkSync, writeFileSync } from "node:fs"
import { join } from "node:path"

const FILE_NAME = "api-keys.json"

export class CredentialStore {
  private readonly path: string

  constructor(userDataPath: string) {
    this.path = join(userDataPath, FILE_NAME)
  }

  loadAll(): Record<string, string> {
    if (!safeStorage.isEncryptionAvailable()) return {}
    if (!existsSync(this.path)) return {}

    let stored: Record<string, string>
    try {
      stored = JSON.parse(readFileSync(this.path, "utf-8")) as Record<
        string,
        string
      >
    } catch {
      return {}
    }

    const result: Record<string, string> = {}
    for (const [slot, encoded] of Object.entries(stored)) {
      if (typeof encoded !== "string") continue
      try {
        result[slot] = safeStorage.decryptString(Buffer.from(encoded, "base64"))
      } catch {
        // Entry corrupted or re-keyed — drop silently.
      }
    }
    return result
  }

  save(entries: Record<string, string | null>): void {
    if (!safeStorage.isEncryptionAvailable()) return

    const current = this.loadRaw()
    for (const [slot, value] of Object.entries(entries)) {
      if (value && value.trim()) {
        current[slot] = safeStorage
          .encryptString(value)
          .toString("base64")
      } else {
        delete current[slot]
      }
    }

    if (Object.keys(current).length === 0) {
      try {
        unlinkSync(this.path)
      } catch {
        // File already absent.
      }
      return
    }

    writeFileSync(this.path, JSON.stringify(current), "utf-8")
  }

  private loadRaw(): Record<string, string> {
    try {
      return JSON.parse(readFileSync(this.path, "utf-8")) as Record<
        string,
        string
      >
    } catch {
      return {}
    }
  }
}
