import type { SubtitleCue, SubtitlePosition } from "@/features/projects/types"

type CueGeometry = SubtitleCue["geometry"]

export type CueImagePlacement = {
  canvasWidth: number
  canvasHeight: number
  x: number
  y: number
  width: number
  height: number
}

export type CueSubtitlePlacement = CueImagePlacement & {
  fontSize: number
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

export function cueSubtitlePlacement(
  geometry: CueGeometry,
  sourcePosition: SubtitlePosition,
  draftPosition: SubtitlePosition,
  lineCount: number
): CueSubtitlePlacement {
  const canvasWidth = Math.max(1, geometry.canvas_width)
  const canvasHeight = Math.max(1, geometry.canvas_height)
  const width = Math.max(1, geometry.width)
  const height = Math.max(1, geometry.height)
  const unchanged = sourcePosition === draftPosition
  const [vertical, horizontal] = draftPosition.split("-") as [
    "top" | "middle" | "bottom",
    "left" | "center" | "right",
  ]
  const horizontalMargin = Math.max(32, canvasWidth * 0.035)
  const verticalMargin = Math.max(24, canvasHeight * 0.04)
  const x = unchanged
    ? geometry.x
    : horizontal === "left"
      ? horizontalMargin
      : horizontal === "right"
        ? canvasWidth - horizontalMargin - width
        : (canvasWidth - width) / 2
  const y = unchanged
    ? geometry.y
    : vertical === "top"
      ? verticalMargin
      : vertical === "bottom"
        ? canvasHeight - verticalMargin - height
        : (canvasHeight - height) / 2
  const rows = Math.max(1, lineCount)
  const lineHeight = height / rows

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
