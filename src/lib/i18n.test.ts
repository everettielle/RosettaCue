import { describe, expect, it } from "vitest"

import * as m from "@/paraglide/messages.js"
import { baseLocale, getTextDirection, locales } from "@/paraglide/runtime.js"

describe("i18n", () => {
  it("configures English as the only locale", () => {
    expect(baseLocale).toBe("en")
    expect(locales).toEqual(["en"])
    expect(getTextDirection("en")).toBe("ltr")
  })

  it("renders parameterized English messages", () => {
    expect(m.status_cue_saved({ index: 42 })).toBe("Cue 42 saved")
    expect(m.welcome_version({ version: "0.1.0" })).toBe("Version 0.1.0")
  })
})
