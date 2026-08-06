import type { OcrLine, OcrSpan, TextStyle } from "@/features/projects/types"

export const textStyleOrder: TextStyle[] = [
  "bold",
  "italic",
  "underline",
  "strikethrough",
  "superscript",
  "subscript",
]

export function canonicalStyles(styles: Iterable<TextStyle>) {
  const values = new Set(styles)
  return textStyleOrder.filter((style) => values.has(style))
}

function sameStyles(left: TextStyle[], right: TextStyle[]) {
  return (
    left.length === right.length &&
    left.every((style, index) => style === right[index])
  )
}

export function canonicalColor(color: string | null | undefined) {
  if (!color || !/^#[0-9a-f]{6}$/i.test(color)) return null
  const normalized = color.toUpperCase()
  return normalized === "#FFFFFF" ? null : normalized
}

export function appendCanonicalSpan(target: OcrSpan[], span: OcrSpan) {
  if (span.type === "text" && span.text.length === 0) return

  const color = canonicalColor(span.color)
  const normalized: OcrSpan =
    span.type === "text"
      ? {
          type: "text",
          text: span.text,
          styles: canonicalStyles(span.styles),
          ...(color ? { color } : {}),
        }
      : {
          type: "ruby",
          base: span.base,
          annotations: span.annotations.map((annotation) => ({
            ...annotation,
          })),
          styles: canonicalStyles(span.styles),
          ...(color ? { color } : {}),
        }
  const previous = target.at(-1)
  if (
    previous?.type === "text" &&
    normalized.type === "text" &&
    sameStyles(previous.styles, normalized.styles) &&
    canonicalColor(previous.color) === canonicalColor(normalized.color)
  ) {
    previous.text += normalized.text
    return
  }
  target.push(normalized)
}

export function normalizeOcrLines(lines: OcrLine[]) {
  return lines.map((line) => {
    const spans: OcrSpan[] = []
    line.spans.forEach((span) => appendCanonicalSpan(spans, span))
    return {
      ...line,
      text: spans
        .map((span) => (span.type === "text" ? span.text : span.base))
        .join(""),
      spans,
    }
  })
}
