import type { OcrDocument } from "@/features/projects/types"
import { cn } from "@/lib/utils"
import * as m from "@/paraglide/messages.js"

export function RenderedSubtitle({
  document,
  fontSize,
  lineHeight,
  appearance = "preview",
  wrap = false,
}: {
  document: OcrDocument | null
  fontSize: number
  lineHeight: number
  appearance?: "preview" | "document"
  wrap?: boolean
}) {
  if (!document) {
    return (
      <p
        className={cn(
          appearance === "preview"
            ? "text-preview-muted"
            : "text-muted-foreground"
        )}
        style={{ fontSize }}
      >
        {m.workspace_no_ocr_text()}
      </p>
    )
  }

  return (
    <div
      className={cn(
        "flex w-full flex-col",
        appearance === "preview"
          ? "text-preview-foreground [text-shadow:0_1px_2px_var(--preview-shadow),0_0_1px_var(--preview-shadow)]"
          : "text-card-foreground"
      )}
    >
      {document.lines.map((line, lineIndex) => (
        <p
          key={`${line.text}-${lineIndex}`}
          className={cn(
            "m-0 font-medium",
            wrap ? "whitespace-pre-wrap" : "whitespace-pre"
          )}
          style={{ fontSize, lineHeight: `${lineHeight}px` }}
        >
          {line.spans.length === 0 ? (
            <span>{line.text}</span>
          ) : (
            line.spans.map((span, spanIndex) =>
              span.type === "text" ? (
                <span
                  key={spanIndex}
                  style={{ color: span.color ?? undefined }}
                  className={cn(
                    span.styles.includes("bold") && "font-bold",
                    span.styles.includes("italic") && "italic",
                    span.styles.includes("underline") && "underline",
                    span.styles.includes("strikethrough") && "line-through",
                    span.styles.includes("superscript") &&
                      "align-super text-[0.75em] leading-none",
                    span.styles.includes("subscript") &&
                      "align-sub text-[0.75em] leading-none"
                  )}
                >
                  {span.text}
                </span>
              ) : (
                <ruby
                  key={spanIndex}
                  style={{
                    rubyPosition:
                      span.annotations[0]?.position === "under"
                        ? "under"
                        : "over",
                    color: span.color ?? undefined,
                  }}
                  className={cn(
                    span.styles.includes("bold") && "font-bold",
                    span.styles.includes("italic") && "italic",
                    span.styles.includes("underline") && "underline",
                    span.styles.includes("strikethrough") && "line-through",
                    span.styles.includes("superscript") &&
                      "align-super text-[0.75em] leading-none",
                    span.styles.includes("subscript") &&
                      "align-sub text-[0.75em] leading-none"
                  )}
                >
                  {span.base}
                  {span.annotations.map((annotation, annotationIndex) => (
                    <rt key={annotationIndex}>{annotation.text}</rt>
                  ))}
                </ruby>
              )
            )
          )}
        </p>
      ))}
    </div>
  )
}
