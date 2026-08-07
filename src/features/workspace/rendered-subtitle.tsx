import type { OcrDocument, OcrLine, TextBlock } from "@/features/projects/types"
import { cn } from "@/lib/utils"
import * as m from "@/paraglide/messages.js"

type Appearance = "preview" | "document"

/**
 * Renders one text block.
 *
 * A vertical block is the same markup under `writing-mode: vertical-rl`: the
 * browser then stacks its columns right to left and, because ruby positions are
 * stored relative to the writing direction, places `over` annotations to the
 * right of the column without any of this code knowing.
 */
export function RenderedBlock({
  block,
  fontSize,
  lineHeight,
  appearance = "preview",
  wrap = false,
}: {
  block: TextBlock
  fontSize: number
  lineHeight: number
  appearance?: Appearance
  wrap?: boolean
}) {
  const vertical = block.writing_mode === "vertical_rl"
  return (
    <div
      className={cn(
        appearanceClasses(appearance),
        vertical ? "inline-block" : "flex flex-col"
      )}
      // In vertical-rl the paragraphs are block boxes stacked along the block
      // axis, which now runs right to left — no reordering of our own needed.
      style={vertical ? { writingMode: "vertical-rl" } : undefined}
    >
      {block.lines.map((line, lineIndex) => (
        <p
          key={`${line.text}-${lineIndex}`}
          className={cn(
            "m-0 font-medium",
            wrap ? "whitespace-pre-wrap" : "whitespace-pre"
          )}
          style={{ fontSize, lineHeight: `${lineHeight}px` }}
        >
          <LineSpans line={line} />
        </p>
      ))}
    </div>
  )
}

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
  appearance?: Appearance
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
    <div className="flex w-full flex-col gap-2">
      {document.blocks.map((block, blockIndex) => (
        <RenderedBlock
          key={blockIndex}
          block={block}
          fontSize={fontSize}
          lineHeight={lineHeight}
          appearance={appearance}
          wrap={wrap}
        />
      ))}
    </div>
  )
}

function appearanceClasses(appearance: Appearance) {
  return appearance === "preview"
    ? "text-preview-foreground [text-shadow:0_1px_2px_var(--preview-shadow),0_0_1px_var(--preview-shadow)]"
    : "text-card-foreground"
}

function LineSpans({ line }: { line: OcrLine }) {
  if (line.spans.length === 0) {
    return <span>{line.text}</span>
  }
  return line.spans.map((span, spanIndex) =>
    span.type === "text" ? (
      <span
        key={spanIndex}
        style={{ color: span.color ?? undefined }}
        className={spanStyles(span.styles)}
      >
        {span.text}
      </span>
    ) : (
      <ruby
        key={spanIndex}
        style={{
          rubyPosition:
            span.annotations[0]?.position === "under" ? "under" : "over",
          color: span.color ?? undefined,
        }}
        className={spanStyles(span.styles)}
      >
        {span.base}
        {span.annotations.map((annotation, annotationIndex) => (
          <rt key={annotationIndex}>{annotation.text}</rt>
        ))}
      </ruby>
    )
  )
}

function spanStyles(styles: OcrLine["spans"][number]["styles"]) {
  return cn(
    styles.includes("bold") && "font-bold",
    styles.includes("italic") && "italic",
    styles.includes("underline") && "underline",
    styles.includes("strikethrough") && "line-through",
    styles.includes("superscript") && "align-super text-[0.75em] leading-none",
    styles.includes("subscript") && "align-sub text-[0.75em] leading-none"
  )
}
