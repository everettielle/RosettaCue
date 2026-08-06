import { describe, expect, it } from "vitest"

import { sanitizeDiagnosticValue } from "./diagnostics"

describe("diagnostic redaction", () => {
  it("redacts credentials and binary payloads without dropping response fields", () => {
    const value = sanitizeDiagnosticValue({
      api_key: "secret",
      headers: { Authorization: "Bearer secret" },
      request: { image: "data:image/png;base64,cG5n" },
      response: {
        status: 200,
        body: JSON.stringify({
          choices: [
            { message: { content: "", reasoning_content: "full result" } },
          ],
        }),
      },
      bytes: Array.from({ length: 300 }, (_, index) => index % 256),
    }) as Record<string, unknown>

    expect(value.api_key).toBe("[REDACTED]")
    expect(value.headers).toEqual({ Authorization: "[REDACTED]" })
    expect(value.request).toEqual({
      image: { redacted: "base64_data", estimated_byte_length: 3 },
    })
    expect(JSON.stringify(value.response)).toContain("reasoning_content")
    expect(value.bytes).toEqual({
      redacted: "binary_array",
      byte_length: 300,
    })
  })
})
