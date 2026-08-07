import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import type { OcrDocument, TextBlock } from "@/features/projects/types"
import { RenderedSubtitle } from "@/features/workspace/rendered-subtitle"

const rubyBlock: TextBlock = {
  bounds: { x: 476, y: 730, width: 1004, height: 203 },
  writing_mode: "horizontal_tb",
  position: "bottom-center",
  source: "detected",
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
}

const document: OcrDocument = {
  prompt_version: "test",
  provider: "test",
  model: "test",
  language: "jpn",
  unreadable: false,
  blocks: [rubyBlock],
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

  it("hands a vertical block to CSS and keeps the ruby position relative", () => {
    const markup = renderToStaticMarkup(
      <RenderedSubtitle
        document={{
          ...document,
          blocks: [{ ...rubyBlock, writing_mode: "vertical_rl" }],
        }}
        fontSize={16}
        lineHeight={32}
      />
    )

    // "over" stays "over": in vertical-rl the browser reads that as the
    // right-hand side of the column, which is where furigana belongs.
    expect(markup).toContain("writing-mode:vertical-rl")
    expect(markup).toContain("ruby-position:over")
  })

  it("renders each block of a multi-block cue", () => {
    const markup = renderToStaticMarkup(
      <RenderedSubtitle
        document={{
          ...document,
          blocks: [
            {
              ...rubyBlock,
              writing_mode: "vertical_rl",
              lines: [{ text: "冷たい！", spans: [] }],
            },
            { ...rubyBlock, lines: [{ text: "おもいけるかな", spans: [] }] },
          ],
        }}
        fontSize={16}
        lineHeight={32}
      />
    )

    expect(markup).toContain("冷たい！")
    expect(markup).toContain("おもいけるかな")
  })
})
