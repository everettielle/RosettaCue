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

export function appendCanonicalSpan(target: OcrSpan[], span: OcrSpan) {
  if (span.type === "text" && span.text.length === 0) return

  const normalized: OcrSpan =
    span.type === "text"
      ? {
          type: "text",
          text: span.text,
          styles: canonicalStyles(span.styles),
        }
      : {
          type: "ruby",
          base: span.base,
          annotations: span.annotations.map((annotation) => ({
            ...annotation,
          })),
          styles: canonicalStyles(span.styles),
        }
  const previous = target.at(-1)
  if (
    previous?.type === "text" &&
    normalized.type === "text" &&
    sameStyles(previous.styles, normalized.styles)
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
