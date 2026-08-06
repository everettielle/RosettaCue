import { describe, expect, it } from "vitest"

import type { OcrLine } from "@/features/projects/types"
import { normalizeOcrLines } from "@/features/workspace/subtitle-spans"

describe("normalizeOcrLines", () => {
  it("maximally merges adjacent text spans with the same style set", () => {
    const lines: OcrLine[] = [
      {
        text: "＂Voices＂によって創造されたー",
        spans: [
          {
            type: "text",
            text: "＂Voices＂によって創造",
            styles: ["italic"],
          },
          {
            type: "text",
            text: "され",
            styles: ["italic", "bold"],
          },
          {
            type: "text",
            text: "たー",
            styles: ["italic"],
          },
        ],
      },
    ]

    lines[0].spans[1] = {
      type: "text",
      text: "され",
      styles: ["italic"],
    }

    expect(normalizeOcrLines(lines)).toEqual([
      {
        text: "＂Voices＂によって創造されたー",
        spans: [
          {
            type: "text",
            text: "＂Voices＂によって創造されたー",
            styles: ["italic"],
          },
        ],
      },
    ])
  })

  it("canonicalizes style order without merging across ruby", () => {
    expect(
      normalizeOcrLines([
        {
          text: "司る",
          spans: [
            {
              type: "ruby",
              base: "司",
              annotations: [{ text: "つかさど", position: "over" }],
              styles: ["subscript", "bold", "italic"],
            },
            { type: "text", text: "る", styles: ["italic", "bold"] },
          ],
        },
      ])[0].spans
    ).toEqual([
      {
        type: "ruby",
        base: "司",
        annotations: [{ text: "つかさど", position: "over" }],
        styles: ["bold", "italic", "subscript"],
      },
      { type: "text", text: "る", styles: ["bold", "italic"] },
    ])
  })

  it("canonicalizes colors and only merges spans with the same color", () => {
    const [line] = normalizeOcrLines([
      {
        text: "RGB",
        spans: [
          { type: "text", text: "R", styles: [], color: "#ff0000" },
          { type: "text", text: "G", styles: [], color: "#00ff00" },
          { type: "text", text: "B", styles: [], color: "#00FF00" },
        ],
      },
    ])

    expect(line.spans).toEqual([
      { type: "text", text: "R", styles: [], color: "#FF0000" },
      { type: "text", text: "GB", styles: [], color: "#00FF00" },
    ])
  })
})
