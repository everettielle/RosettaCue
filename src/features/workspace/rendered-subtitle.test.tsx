import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import type { OcrDocument } from "@/features/projects/types"
import { RenderedSubtitle } from "@/features/workspace/rendered-subtitle"

const document: OcrDocument = {
  prompt_version: "test",
  provider: "test",
  model: "test",
  language: "jpn",
  unreadable: false,
  lines: [
    {
      text: "千早前",
      spans: [
        {
          type: "ruby",
          base: "千早",
          annotations: [{ text: "ちはや", position: "over" }],
          styles: ["bold"],
          color: "#FF0000",
        },
        { type: "text", text: "前", styles: ["italic"] },
      ],
    },
  ],
  normalizations: [],
}

describe("RenderedSubtitle", () => {
  it("renders the full ruby, style, and color structure", () => {
    const markup = renderToStaticMarkup(
      <RenderedSubtitle
        document={document}
        fontSize={16}
        lineHeight={32}
        appearance="document"
        wrap
      />
    )

    expect(markup).toContain("<ruby")
    expect(markup).toContain("<rt>ちはや</rt>")
    expect(markup).toContain("ruby-position:over")
    expect(markup).toContain("color:#FF0000")
    expect(markup).toContain("font-bold")
    expect(markup).toContain("italic")
    expect(markup).toContain("whitespace-pre-wrap")
  })
})
