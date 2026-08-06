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
})
