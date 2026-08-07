import type { SubtitleCue, TextBlock } from "@/features/projects/types"

type CueGeometry = SubtitleCue["geometry"]

export type CueImagePlacement = {
  canvasWidth: number
  canvasHeight: number
  x: number
  y: number
  width: number
  height: number
}

export type BlockPlacement = CueImagePlacement & {
  fontSize: number
  /** Line spacing in horizontal writing, column spacing in vertical writing. */
  lineHeight: number
  alignItems: "flex-start" | "center" | "flex-end"
  textAlign: "left" | "center" | "right"
}

export function cueImagePlacement(geometry: CueGeometry): CueImagePlacement {
  const horizontalPadding = Math.max(
    0,
    (geometry.image_width - geometry.width) / 2
  )
  const verticalPadding = Math.max(
    0,
    (geometry.image_height - geometry.height) / 2
  )

  return {
    canvasWidth: Math.max(1, geometry.canvas_width),
    canvasHeight: Math.max(1, geometry.canvas_height),
    x: geometry.x - horizontalPadding,
    y: geometry.y - verticalPadding,
    width: geometry.image_width,
    height: geometry.image_height,
  }
}

/**
 * Places one text block on the preview canvas.
 *
 * While the block sits where it was recognized, its own bounds are the truth
 * and are used as-is. Once someone moves it to a different cell of the 3×3
 * grid there is no measured rectangle to fall back on, so the same-sized box is
 * placed against the canvas margins instead — which is why the caller passes
 * whether the position was edited rather than the placement re-deriving it.
 */
export function blockSubtitlePlacement(
  geometry: CueGeometry,
  block: TextBlock,
  moved: boolean,
  unitCount: number
): BlockPlacement {
  const canvasWidth = Math.max(1, geometry.canvas_width)
  const canvasHeight = Math.max(1, geometry.canvas_height)
  const width = Math.max(1, block.bounds.width)
  const height = Math.max(1, block.bounds.height)
  const [vertical, horizontal] = block.position.split("-") as [
    "top" | "middle" | "bottom",
    "left" | "center" | "right",
  ]
  const horizontalMargin = Math.max(32, canvasWidth * 0.035)
  const verticalMargin = Math.max(24, canvasHeight * 0.04)
  const x = !moved
    ? block.bounds.x
    : horizontal === "left"
      ? horizontalMargin
      : horizontal === "right"
        ? canvasWidth - horizontalMargin - width
        : (canvasWidth - width) / 2
  const y = !moved
    ? block.bounds.y
    : vertical === "top"
      ? verticalMargin
      : vertical === "bottom"
        ? canvasHeight - verticalMargin - height
        : (canvasHeight - height) / 2
  const units = Math.max(1, unitCount)
  // Units stack across the flow: rows down the block's height in horizontal
  // writing, columns across its width in vertical writing.
  const lineHeight =
    block.writing_mode === "vertical_rl" ? width / units : height / units

  return {
    canvasWidth,
    canvasHeight,
    x: clamp(x, 0, Math.max(0, canvasWidth - width)),
    y: clamp(y, 0, Math.max(0, canvasHeight - height)),
    width,
    height,
    fontSize: Math.max(12, lineHeight * 0.6),
    lineHeight,
    alignItems:
      horizontal === "left"
        ? "flex-start"
        : horizontal === "right"
          ? "flex-end"
          : "center",
    textAlign: horizontal,
  }
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value))
}
