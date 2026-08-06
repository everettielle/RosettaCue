import { describe, expect, it } from "vitest"

import {
  cueImagePlacement,
  cueSubtitlePlacement,
} from "@/features/workspace/cue-geometry"

describe("cueImagePlacement", () => {
  it("places the padded Cue image at its original Blu-ray canvas position", () => {
    expect(
      cueImagePlacement({
        canvas_width: 1920,
        canvas_height: 1080,
        x: 700,
        y: 850,
        width: 520,
        height: 80,
        image_width: 552,
        image_height: 112,
        forced: false,
        inferred_end: false,
      })
    ).toEqual({
      canvasWidth: 1920,
      canvasHeight: 1080,
      x: 684,
      y: 834,
      width: 552,
      height: 112,
    })
  })

  it("does not offset an image without extraction padding", () => {
    expect(
      cueImagePlacement({
        canvas_width: 1280,
        canvas_height: 720,
        x: 120,
        y: 90,
        width: 400,
        height: 60,
        image_width: 400,
        image_height: 60,
        forced: false,
        inferred_end: false,
      })
    ).toMatchObject({ x: 120, y: 90, width: 400, height: 60 })
  })
})

describe("cueSubtitlePlacement", () => {
  const geometry = {
    canvas_width: 1920,
    canvas_height: 1080,
    x: 476,
    y: 730,
    width: 1004,
    height: 203,
    image_width: 1036,
    image_height: 235,
    forced: false,
    inferred_end: false,
  }

  it("uses the source bounding box and canvas for an unchanged position", () => {
    expect(
      cueSubtitlePlacement(geometry, "bottom-center", "bottom-center", 2)
    ).toMatchObject({
      canvasWidth: 1920,
      canvasHeight: 1080,
      x: 476,
      y: 730,
      width: 1004,
      height: 203,
      alignItems: "center",
      textAlign: "center",
    })
  })

  it("moves the same-sized box when the semantic position is edited", () => {
    const placement = cueSubtitlePlacement(
      geometry,
      "bottom-center",
      "top-left",
      2
    )
    expect(placement.x).toBeCloseTo(67.2)
    expect(placement.y).toBeCloseTo(43.2)
    expect(placement.width).toBe(1004)
    expect(placement.height).toBe(203)
    expect(placement.alignItems).toBe("flex-start")
  })
})
