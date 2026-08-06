import * as React from "react"
import {
  BoldIcon,
  CaptionsIcon,
  EraserIcon,
  ItalicIcon,
  PaletteIcon,
  StrikethroughIcon,
  SubscriptIcon,
  SuperscriptIcon,
  UnderlineIcon,
  type LucideIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { Toggle } from "@/components/ui/toggle"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import type { OcrLine, OcrSpan, TextStyle } from "@/features/projects/types"
import {
  appendCanonicalSpan,
  canonicalColor,
  canonicalStyles,
  normalizeOcrLines,
  textStyleOrder,
} from "@/features/workspace/subtitle-spans"
import { cn } from "@/lib/utils"
import * as m from "@/paraglide/messages.js"

type EditorCommand = {
  style: TextStyle
  label: string
  icon: LucideIcon
}

const editorCommands: EditorCommand[] = [
  { style: "bold", label: m.editor_bold(), icon: BoldIcon },
  { style: "italic", label: m.editor_italic(), icon: ItalicIcon },
  {
    style: "underline",
    label: m.editor_underline(),
    icon: UnderlineIcon,
  },
  {
    style: "strikethrough",
    label: m.editor_strikethrough(),
    icon: StrikethroughIcon,
  },
  {
    style: "superscript",
    label: m.editor_superscript(),
    icon: SuperscriptIcon,
  },
  {
    style: "subscript",
    label: m.editor_subscript(),
    icon: SubscriptIcon,
  },
]

function stylesForElement(element: Element, inherited: TextStyle[]) {
  const styles = new Set(inherited)
  const tag = element.tagName.toLowerCase()
  if (tag === "b" || tag === "strong") styles.add("bold")
  if (tag === "i" || tag === "em") styles.add("italic")
  if (tag === "u") styles.add("underline")
  if (tag === "s" || tag === "strike" || tag === "del") {
    styles.add("strikethrough")
  }
  if (tag === "sup") {
    styles.delete("subscript")
    styles.add("superscript")
  }
  if (tag === "sub") {
    styles.delete("superscript")
    styles.add("subscript")
  }

  const computed = window.getComputedStyle(element)
  if (Number.parseInt(computed.fontWeight, 10) >= 600) styles.add("bold")
  if (computed.fontStyle === "italic" || computed.fontStyle === "oblique") {
    styles.add("italic")
  }
  if (computed.textDecorationLine.includes("underline")) {
    styles.add("underline")
  }
  if (computed.textDecorationLine.includes("line-through")) {
    styles.add("strikethrough")
  }
  return canonicalStyles(styles)
}

function colorForElement(element: Element, inherited: string | null) {
  return canonicalColor(element.getAttribute("data-text-color")) ?? inherited
}

function textWithoutRubyAnnotations(node: Node): string {
  if (node instanceof Element && node.tagName.toLowerCase() === "rt") return ""
  return Array.from(node.childNodes)
    .map((child) =>
      child.nodeType === Node.TEXT_NODE
        ? (child.textContent ?? "")
        : textWithoutRubyAnnotations(child)
    )
    .join("")
}

function collectStyles(
  node: Node,
  inherited: TextStyle[],
  output: Set<TextStyle>
) {
  if (!(node instanceof Element)) return
  if (node.tagName.toLowerCase() === "rt") return
  const next = stylesForElement(node, inherited)
  next.forEach((style) => output.add(style))
  node.childNodes.forEach((child) => collectStyles(child, next, output))
}

function collectSpans(
  node: Node,
  inherited: TextStyle[],
  inheritedColor: string | null,
  target: OcrSpan[]
) {
  if (node.nodeType === Node.TEXT_NODE) {
    appendCanonicalSpan(target, {
      type: "text",
      text: (node.textContent ?? "").replaceAll("\u200B", ""),
      styles: canonicalStyles(inherited),
      color: inheritedColor,
    })
    return
  }
  if (!(node instanceof Element)) return

  const tag = node.tagName.toLowerCase()
  if (tag === "rt" || tag === "br") return
  const nextStyles = stylesForElement(node, inherited)
  const nextColor = colorForElement(node, inheritedColor)
  if (tag === "ruby") {
    const base = textWithoutRubyAnnotations(node).replaceAll("\u200B", "")
    const annotations = Array.from(node.querySelectorAll(":scope > rt")).map(
      (annotation) => ({
        text: annotation.textContent ?? "",
        position:
          annotation.getAttribute("data-position") === "under"
            ? ("under" as const)
            : ("over" as const),
      })
    )
    const rubyStyles = new Set(nextStyles)
    node.childNodes.forEach((child) =>
      collectStyles(child, nextStyles, rubyStyles)
    )
    if (base.length > 0 && annotations.every((annotation) => annotation.text)) {
      target.push({
        type: "ruby",
        base,
        annotations,
        styles: canonicalStyles(rubyStyles),
        color: nextColor,
      })
    } else {
      appendCanonicalSpan(target, {
        type: "text",
        text: base,
        styles: nextStyles,
        color: nextColor,
      })
    }
    return
  }

  node.childNodes.forEach((child) =>
    collectSpans(child, nextStyles, nextColor, target)
  )
}

function lineFromNode(node: Node): OcrLine {
  const spans: OcrSpan[] = []
  collectSpans(node, [], null, spans)
  const text = spans
    .map((span) => (span.type === "text" ? span.text : span.base))
    .join("")
  return { text, spans }
}

function parseSubtitleEditor(root: HTMLElement): OcrLine[] {
  const nodes = Array.from(root.childNodes).filter(
    (node) =>
      node.nodeType !== Node.TEXT_NODE || Boolean(node.textContent?.length)
  )
  if (nodes.length === 0) return []
  return nodes.map(lineFromNode)
}

function appendStyledContent(parent: HTMLElement, span: OcrSpan) {
  let content: HTMLElement | Text =
    span.type === "text"
      ? document.createTextNode(span.text)
      : createRubyElement(span)

  for (const style of textStyleOrder.toReversed()) {
    if (!span.styles.includes(style)) continue
    const element = document.createElement(
      style === "bold"
        ? "strong"
        : style === "italic"
          ? "em"
          : style === "underline"
            ? "u"
            : style === "strikethrough"
              ? "s"
              : style === "superscript"
                ? "sup"
                : "sub"
    )
    element.append(content)
    content = element
  }
  const color = canonicalColor(span.color)
  if (color) {
    const element = document.createElement("span")
    element.dataset.textColor = color
    element.style.color = color
    element.append(content)
    content = element
  }
  parent.append(content)
}

function createRubyElement(span: Extract<OcrSpan, { type: "ruby" }>) {
  const ruby = document.createElement("ruby")
  ruby.style.rubyPosition =
    span.annotations[0]?.position === "under" ? "under" : "over"
  ruby.append(document.createTextNode(span.base))
  for (const annotation of span.annotations) {
    const rt = document.createElement("rt")
    rt.contentEditable = "false"
    rt.dataset.position = annotation.position
    rt.textContent = annotation.text
    ruby.append(rt)
  }
  return ruby
}

function writeLines(root: HTMLElement, lines: OcrLine[]) {
  const elements = lines.map((line) => {
    const element = document.createElement("div")
    element.dataset.editorLine = ""
    if (line.spans.length === 0) {
      element.append(document.createElement("br"))
    } else {
      line.spans.forEach((span) => appendStyledContent(element, span))
    }
    return element
  })
  root.replaceChildren(...elements)
}

type EditorSelection = {
  startLine: number
  startOffset: number
  endLine: number
  endOffset: number
}

function selectionPoint(root: HTMLElement, node: Node, offset: number) {
  const lineElements = Array.from(root.children)
  if (node === root) {
    if (lineElements.length === 0) return null
    if (offset <= 0) return { line: 0, offset: 0 }
    if (offset >= lineElements.length) {
      const line = lineElements.length - 1
      return {
        line,
        offset: textWithoutRubyAnnotations(lineElements[line]).length,
      }
    }
    return { line: offset, offset: 0 }
  }
  const line = lineElements.findIndex(
    (candidate) => candidate === node || candidate.contains(node)
  )
  if (line < 0) return null
  const range = document.createRange()
  range.setStart(lineElements[line], 0)
  range.setEnd(node, offset)
  return {
    line,
    offset: textWithoutRubyAnnotations(range.cloneContents()).length,
  }
}

function captureSelection(
  root: HTMLElement,
  selection: Selection
): EditorSelection | null {
  if (selection.rangeCount === 0) return null
  const range = selection.getRangeAt(0)
  const start = selectionPoint(root, range.startContainer, range.startOffset)
  const end = selectionPoint(root, range.endContainer, range.endOffset)
  if (!start || !end) return null
  return {
    startLine: start.line,
    startOffset: start.offset,
    endLine: end.line,
    endOffset: end.offset,
  }
}

function selectedSpanStyles(lines: OcrLine[], selection: EditorSelection) {
  let intersection: Set<TextStyle> | null = null
  lines.forEach((line, lineIndex) => {
    if (lineIndex < selection.startLine || lineIndex > selection.endLine) return
    const selectedStart =
      lineIndex === selection.startLine ? selection.startOffset : 0
    const selectedEnd =
      lineIndex === selection.endLine ? selection.endOffset : line.text.length
    let cursor = 0
    line.spans.forEach((span) => {
      const text = span.type === "text" ? span.text : span.base
      const end = cursor + text.length
      if (selectedStart < end && selectedEnd > cursor) {
        const styles = new Set(span.styles)
        intersection =
          intersection === null
            ? styles
            : new Set([...intersection].filter((style) => styles.has(style)))
      }
      cursor = end
    })
  })
  return intersection ?? new Set<TextStyle>()
}

function selectedSpanColor(lines: OcrLine[], selection: EditorSelection) {
  const colors = new Set<string | null>()
  lines.forEach((line, lineIndex) => {
    if (lineIndex < selection.startLine || lineIndex > selection.endLine) return
    const selectedStart =
      lineIndex === selection.startLine ? selection.startOffset : 0
    const selectedEnd =
      lineIndex === selection.endLine ? selection.endOffset : line.text.length
    let cursor = 0
    line.spans.forEach((span) => {
      const text = span.type === "text" ? span.text : span.base
      const end = cursor + text.length
      if (selectedStart < end && selectedEnd > cursor) {
        colors.add(canonicalColor(span.color))
      }
      cursor = end
    })
  })
  return colors.size === 1 ? [...colors][0] : undefined
}

function selectionHasExplicitColor(
  lines: OcrLine[],
  selection: EditorSelection
) {
  return lines.some((line, lineIndex) => {
    if (lineIndex < selection.startLine || lineIndex > selection.endLine) {
      return false
    }
    const selectedStart =
      lineIndex === selection.startLine ? selection.startOffset : 0
    const selectedEnd =
      lineIndex === selection.endLine ? selection.endOffset : line.text.length
    let cursor = 0
    return line.spans.some((span) => {
      const text = span.type === "text" ? span.text : span.base
      const end = cursor + text.length
      const overlaps = selectedStart < end && selectedEnd > cursor
      cursor = end
      return overlaps && Boolean(canonicalColor(span.color))
    })
  })
}

function withToggledStyle(
  styles: TextStyle[],
  style: TextStyle,
  remove: boolean
) {
  const next = new Set(styles)
  if (remove) next.delete(style)
  else {
    if (style === "superscript") next.delete("subscript")
    if (style === "subscript") next.delete("superscript")
    next.add(style)
  }
  return canonicalStyles(next)
}

function formatLineRange(
  line: OcrLine,
  selectedStart: number,
  selectedEnd: number,
  style: TextStyle,
  remove: boolean
) {
  const spans: OcrSpan[] = []
  let cursor = 0
  for (const span of line.spans) {
    const text = span.type === "text" ? span.text : span.base
    const end = cursor + text.length
    const overlapStart = Math.max(selectedStart, cursor)
    const overlapEnd = Math.min(selectedEnd, end)
    if (overlapStart >= overlapEnd) {
      appendCanonicalSpan(spans, structuredClone(span))
      cursor = end
      continue
    }
    if (span.type === "ruby") {
      spans.push({
        ...structuredClone(span),
        styles: withToggledStyle(span.styles, style, remove),
      })
      cursor = end
      continue
    }
    const localStart = overlapStart - cursor
    const localEnd = overlapEnd - cursor
    appendCanonicalSpan(spans, {
      type: "text",
      text: text.slice(0, localStart),
      styles: [...span.styles],
      color: span.color,
    })
    appendCanonicalSpan(spans, {
      type: "text",
      text: text.slice(localStart, localEnd),
      styles: withToggledStyle(span.styles, style, remove),
      color: span.color,
    })
    appendCanonicalSpan(spans, {
      type: "text",
      text: text.slice(localEnd),
      styles: [...span.styles],
      color: span.color,
    })
    cursor = end
  }
  return { ...line, spans }
}

function applyStyleToSelection(
  lines: OcrLine[],
  selection: EditorSelection,
  style: TextStyle
) {
  if (
    selection.startLine === selection.endLine &&
    selection.startOffset === selection.endOffset
  ) {
    return lines
  }
  const remove = selectedSpanStyles(lines, selection).has(style)
  return lines.map((line, lineIndex) => {
    if (lineIndex < selection.startLine || lineIndex > selection.endLine) {
      return line
    }
    return formatLineRange(
      line,
      lineIndex === selection.startLine ? selection.startOffset : 0,
      lineIndex === selection.endLine ? selection.endOffset : line.text.length,
      style,
      remove
    )
  })
}

function colorLineRange(
  line: OcrLine,
  selectedStart: number,
  selectedEnd: number,
  color: string | null
) {
  const spans: OcrSpan[] = []
  let cursor = 0
  for (const span of line.spans) {
    const text = span.type === "text" ? span.text : span.base
    const end = cursor + text.length
    const overlapStart = Math.max(selectedStart, cursor)
    const overlapEnd = Math.min(selectedEnd, end)
    if (overlapStart >= overlapEnd) {
      appendCanonicalSpan(spans, structuredClone(span))
      cursor = end
      continue
    }
    if (span.type === "ruby") {
      spans.push({ ...structuredClone(span), color })
      cursor = end
      continue
    }
    const localStart = overlapStart - cursor
    const localEnd = overlapEnd - cursor
    appendCanonicalSpan(spans, {
      type: "text",
      text: text.slice(0, localStart),
      styles: [...span.styles],
      color: span.color,
    })
    appendCanonicalSpan(spans, {
      type: "text",
      text: text.slice(localStart, localEnd),
      styles: [...span.styles],
      color,
    })
    appendCanonicalSpan(spans, {
      type: "text",
      text: text.slice(localEnd),
      styles: [...span.styles],
      color: span.color,
    })
    cursor = end
  }
  return { ...line, spans }
}

function applyColorToSelection(
  lines: OcrLine[],
  selection: EditorSelection,
  color: string | null
) {
  if (
    selection.startLine === selection.endLine &&
    selection.startOffset === selection.endOffset
  ) {
    return lines
  }
  return lines.map((line, lineIndex) => {
    if (lineIndex < selection.startLine || lineIndex > selection.endLine) {
      return line
    }
    return colorLineRange(
      line,
      lineIndex === selection.startLine ? selection.startOffset : 0,
      lineIndex === selection.endLine ? selection.endOffset : line.text.length,
      canonicalColor(color)
    )
  })
}

function rubyAtSelection(lines: OcrLine[], selection: EditorSelection) {
  if (selection.startLine !== selection.endLine) return null
  const line = lines[selection.startLine]
  if (!line) return null
  let cursor = 0
  for (const span of line.spans) {
    const text = span.type === "text" ? span.text : span.base
    const end = cursor + text.length
    if (
      span.type === "ruby" &&
      cursor === selection.startOffset &&
      end === selection.endOffset
    ) {
      return span
    }
    cursor = end
  }
  return null
}

function rubySelectionError(lines: OcrLine[], selection: EditorSelection) {
  if (selection.startLine !== selection.endLine) {
    return m.editor_ruby_one_line_error()
  }
  if (selection.startOffset === selection.endOffset) {
    return m.editor_ruby_select_exact_error()
  }
  const line = lines[selection.startLine]
  if (!line) return m.editor_ruby_missing_line_error()
  let cursor = 0
  for (const span of line.spans) {
    const text = span.type === "text" ? span.text : span.base
    const end = cursor + text.length
    const overlaps = selection.startOffset < end && selection.endOffset > cursor
    if (
      overlaps &&
      span.type === "ruby" &&
      (selection.startOffset !== cursor || selection.endOffset !== end)
    ) {
      return m.editor_ruby_exact_existing_error()
    }
    cursor = end
  }
  return null
}

function applyRubyToSelection(
  lines: OcrLine[],
  selection: EditorSelection,
  annotation: string,
  position: "over" | "under"
) {
  const line = lines[selection.startLine]
  if (!line) return lines
  const base = line.text.slice(selection.startOffset, selection.endOffset)
  const styles = canonicalStyles(selectedSpanStyles(lines, selection))
  const color = selectedSpanColor(lines, selection) ?? null
  const nextSpans: OcrSpan[] = []
  let cursor = 0
  let inserted = false
  for (const span of line.spans) {
    const text = span.type === "text" ? span.text : span.base
    const end = cursor + text.length
    const overlapStart = Math.max(selection.startOffset, cursor)
    const overlapEnd = Math.min(selection.endOffset, end)
    if (overlapStart >= overlapEnd) {
      appendCanonicalSpan(nextSpans, structuredClone(span))
      cursor = end
      continue
    }

    if (span.type === "text" && selection.startOffset > cursor) {
      appendCanonicalSpan(nextSpans, {
        type: "text",
        text: text.slice(0, selection.startOffset - cursor),
        styles: [...span.styles],
        color: span.color,
      })
    }
    if (!inserted) {
      nextSpans.push({
        type: "ruby",
        base,
        annotations: [{ text: annotation, position }],
        styles,
        color,
      })
      inserted = true
    }
    if (span.type === "text" && selection.endOffset < end) {
      appendCanonicalSpan(nextSpans, {
        type: "text",
        text: text.slice(selection.endOffset - cursor),
        styles: [...span.styles],
        color: span.color,
      })
    }
    cursor = end
  }
  return lines.map((candidate, index) =>
    index === selection.startLine
      ? { ...candidate, spans: nextSpans }
      : candidate
  )
}

function removeRubyFromSelection(lines: OcrLine[], selection: EditorSelection) {
  const line = lines[selection.startLine]
  if (!line) return lines
  let cursor = 0
  const spans: OcrSpan[] = []
  for (const span of line.spans) {
    const text = span.type === "text" ? span.text : span.base
    const end = cursor + text.length
    if (
      span.type === "ruby" &&
      cursor === selection.startOffset &&
      end === selection.endOffset
    ) {
      appendCanonicalSpan(spans, {
        type: "text",
        text: span.base,
        styles: [...span.styles],
        color: span.color,
      })
    } else {
      appendCanonicalSpan(spans, structuredClone(span))
    }
    cursor = end
  }
  return lines.map((candidate, index) =>
    index === selection.startLine ? { ...candidate, spans } : candidate
  )
}

function textPointAtOffset(line: Element, targetOffset: number) {
  const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      return node.parentElement?.closest("rt")
        ? NodeFilter.FILTER_REJECT
        : NodeFilter.FILTER_ACCEPT
    },
  })
  let cursor = 0
  let node = walker.nextNode()
  while (node) {
    const length = node.textContent?.length ?? 0
    if (targetOffset <= cursor + length) {
      return { node, offset: targetOffset - cursor }
    }
    cursor += length
    node = walker.nextNode()
  }
  return { node: line, offset: line.childNodes.length }
}

function restoreSelection(root: HTMLElement, selection: EditorSelection) {
  const startLine = root.children[selection.startLine]
  const endLine = root.children[selection.endLine]
  if (!startLine || !endLine) return
  const start = textPointAtOffset(startLine, selection.startOffset)
  const end = textPointAtOffset(endLine, selection.endOffset)
  const range = document.createRange()
  range.setStart(start.node, start.offset)
  range.setEnd(end.node, end.offset)
  const browserSelection = window.getSelection()
  browserSelection?.removeAllRanges()
  browserSelection?.addRange(range)
}

export function SubtitleContentEditor({
  lines,
  disabled,
  onChange,
}: {
  lines: OcrLine[]
  disabled?: boolean
  onChange: (lines: OcrLine[]) => void
}) {
  const rootRef = React.useRef<HTMLDivElement>(null)
  const onChangeRef = React.useRef(onChange)
  const linesRef = React.useRef(lines)
  const lastEmitted = React.useRef("")
  const selectedRange = React.useRef<EditorSelection | null>(null)
  const [activeStyles, setActiveStyles] = React.useState<Set<TextStyle>>(
    new Set()
  )
  const [activeColor, setActiveColor] = React.useState("#FFFFFF")
  const [hasExplicitColor, setHasExplicitColor] = React.useState(false)
  const [rubyOpen, setRubyOpen] = React.useState(false)
  const [rubyBase, setRubyBase] = React.useState("")
  const [rubyText, setRubyText] = React.useState("")
  const [rubyPosition, setRubyPosition] = React.useState<"over" | "under">(
    "over"
  )
  const [rubyError, setRubyError] = React.useState<string | null>(null)
  const [editingRuby, setEditingRuby] = React.useState(false)
  const serializedLines = JSON.stringify(lines)

  React.useLayoutEffect(() => {
    onChangeRef.current = onChange
    linesRef.current = lines
  }, [lines, onChange])

  React.useLayoutEffect(() => {
    const root = rootRef.current
    if (!root || serializedLines === lastEmitted.current) return
    writeLines(root, lines)
    lastEmitted.current = serializedLines
  }, [lines, serializedLines])

  const updateActiveStyles = React.useCallback(() => {
    const root = rootRef.current
    const selection = window.getSelection()
    if (
      !root ||
      !selection?.anchorNode ||
      !root.contains(selection.anchorNode)
    ) {
      return
    }
    const range = captureSelection(root, selection)
    if (!range) return
    selectedRange.current = range
    setActiveStyles(selectedSpanStyles(linesRef.current, range))
    setActiveColor(selectedSpanColor(linesRef.current, range) ?? "#FFFFFF")
    setHasExplicitColor(selectionHasExplicitColor(linesRef.current, range))
  }, [])

  React.useEffect(() => {
    document.addEventListener("selectionchange", updateActiveStyles)
    return () =>
      document.removeEventListener("selectionchange", updateActiveStyles)
  }, [updateActiveStyles])

  const emitChange = () => {
    const root = rootRef.current
    if (!root) return
    const next = normalizeOcrLines(parseSubtitleEditor(root))
    lastEmitted.current = JSON.stringify(next)
    linesRef.current = next
    onChangeRef.current(next)
    updateActiveStyles()
  }

  const commitStructuredLines = (next: OcrLine[], range: EditorSelection) => {
    const root = rootRef.current
    if (!root) return
    const normalized = normalizeOcrLines(next)
    root.focus({ preventScroll: true })
    writeLines(root, normalized)
    restoreSelection(root, range)
    lastEmitted.current = JSON.stringify(normalized)
    linesRef.current = normalized
    onChangeRef.current(normalized)
    setActiveStyles(selectedSpanStyles(normalized, range))
    setActiveColor(selectedSpanColor(normalized, range) ?? "#FFFFFF")
    setHasExplicitColor(selectionHasExplicitColor(normalized, range))
  }

  const applyStyle = (style: TextStyle) => {
    if (disabled) return
    const root = rootRef.current
    const range = selectedRange.current
    if (!root || !range) return
    const next = applyStyleToSelection(linesRef.current, range, style)
    if (next === linesRef.current) return
    commitStructuredLines(next, range)
  }

  const applyColor = (color: string | null) => {
    if (disabled) return
    const root = rootRef.current
    const range = selectedRange.current
    if (!root || !range) return
    const next = applyColorToSelection(linesRef.current, range, color)
    if (next === linesRef.current) return
    commitStructuredLines(next, range)
  }

  const openRubyEditor = () => {
    const range = selectedRange.current
    if (!range) {
      setRubyBase("")
      setRubyText("")
      setEditingRuby(false)
      setRubyError(m.editor_ruby_select_base_error())
      setRubyOpen(true)
      return
    }
    const error = rubySelectionError(linesRef.current, range)
    const existing = rubyAtSelection(linesRef.current, range)
    const line = linesRef.current[range.startLine]
    setRubyBase(
      line && range.startLine === range.endLine
        ? line.text.slice(range.startOffset, range.endOffset)
        : ""
    )
    setRubyText(existing?.annotations[0]?.text ?? "")
    setRubyPosition(existing?.annotations[0]?.position ?? "over")
    setEditingRuby(Boolean(existing))
    setRubyError(error)
    setRubyOpen(true)
  }

  const saveRuby = (event: React.FormEvent) => {
    event.preventDefault()
    const range = selectedRange.current
    if (!range) return
    const error = rubySelectionError(linesRef.current, range)
    const annotation = rubyText.trim()
    if (error) {
      setRubyError(error)
      return
    }
    if (!annotation) {
      setRubyError(m.editor_ruby_enter_annotation_error())
      return
    }
    if (
      Array.from(annotation).some((character) => {
        const codepoint = character.codePointAt(0) ?? 0
        return codepoint < 0x20 || (codepoint >= 0x7f && codepoint <= 0x9f)
      })
    ) {
      setRubyError(m.editor_ruby_control_error())
      return
    }
    const next = applyRubyToSelection(
      linesRef.current,
      range,
      annotation,
      rubyPosition
    )
    commitStructuredLines(next, range)
    setRubyOpen(false)
  }

  const removeRuby = () => {
    const range = selectedRange.current
    if (!range || !rubyAtSelection(linesRef.current, range)) return
    const next = removeRubyFromSelection(linesRef.current, range)
    commitStructuredLines(next, range)
    setRubyOpen(false)
  }

  return (
    <>
      <div className="overflow-hidden rounded-xl border bg-background">
        <div
          className="flex h-10 items-center gap-1 border-b bg-muted/30 px-2"
          role="toolbar"
          aria-label={m.editor_subtitle_formatting()}
        >
          {editorCommands.map(({ style, label, icon: Icon }, index) => (
            <React.Fragment key={style}>
              {(index === 2 || index === 4) && (
                <Separator orientation="vertical" className="mx-1" />
              )}
              <Tooltip>
                <TooltipTrigger
                  render={
                    <Toggle
                      size="sm"
                      pressed={activeStyles.has(style)}
                      disabled={disabled}
                      aria-label={label}
                      onMouseDown={(event) => event.preventDefault()}
                      onPressedChange={() => applyStyle(style)}
                    />
                  }
                >
                  <Icon />
                </TooltipTrigger>
                <TooltipContent>{label}</TooltipContent>
              </Tooltip>
            </React.Fragment>
          ))}
          <Separator orientation="vertical" className="mx-1" />
          <Tooltip>
            <TooltipTrigger
              render={
                <label
                  className={cn(
                    "relative flex size-8 items-center justify-center",
                    disabled
                      ? "cursor-not-allowed opacity-50"
                      : "cursor-pointer"
                  )}
                  aria-label={m.editor_font_color()}
                  aria-disabled={disabled}
                >
                  <PaletteIcon className="size-4" />
                  <Input
                    type="color"
                    value={activeColor}
                    disabled={disabled}
                    className="absolute inset-0 size-full cursor-pointer opacity-0"
                    aria-label={m.editor_font_color()}
                    onChange={(event) => applyColor(event.target.value)}
                  />
                  <span
                    className="pointer-events-none absolute right-1 bottom-0.5 left-1 h-1 rounded-full border"
                    style={{ backgroundColor: activeColor }}
                  />
                </label>
              }
            />
            <TooltipContent>{m.editor_font_color()}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={disabled || !hasExplicitColor}
                  aria-label={m.editor_clear_font_color()}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => applyColor(null)}
                />
              }
            >
              <EraserIcon />
            </TooltipTrigger>
            <TooltipContent>{m.editor_clear_font_color()}</TooltipContent>
          </Tooltip>
          <Separator orientation="vertical" className="mx-1" />
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={disabled}
                  aria-label={m.editor_ruby_annotation()}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={openRubyEditor}
                />
              }
            >
              <CaptionsIcon />
            </TooltipTrigger>
            <TooltipContent>{m.editor_ruby_annotation()}</TooltipContent>
          </Tooltip>
        </div>
        <div
          ref={rootRef}
          contentEditable={!disabled}
          role="textbox"
          aria-label={m.editor_subtitle_content()}
          aria-multiline="true"
          data-placeholder={m.editor_ocr_placeholder()}
          className={cn(
            "min-h-36 px-3 py-2 text-sm leading-6 outline-none",
            "empty:before:pointer-events-none empty:before:text-muted-foreground empty:before:content-[attr(data-placeholder)]",
            disabled && "cursor-not-allowed opacity-50"
          )}
          onInput={emitChange}
          onKeyUp={updateActiveStyles}
          onMouseUp={updateActiveStyles}
          suppressContentEditableWarning
        />
      </div>

      <Dialog open={rubyOpen} onOpenChange={setRubyOpen}>
        <DialogContent>
          <form className="flex flex-col gap-6" onSubmit={saveRuby}>
            <DialogHeader>
              <DialogTitle>
                {editingRuby
                  ? m.editor_edit_ruby_annotation()
                  : m.editor_add_ruby_annotation()}
              </DialogTitle>
              <DialogDescription>{m.editor_description()}</DialogDescription>
            </DialogHeader>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="ruby-base">
                  {m.editor_base_text()}
                </FieldLabel>
                <Input id="ruby-base" value={rubyBase} readOnly />
                <FieldDescription>
                  {m.editor_select_line_description()}
                </FieldDescription>
              </Field>
              <Field data-invalid={Boolean(rubyError) || undefined}>
                <FieldLabel htmlFor="ruby-text">
                  {m.editor_annotation()}
                </FieldLabel>
                <Input
                  id="ruby-text"
                  value={rubyText}
                  autoFocus
                  aria-invalid={Boolean(rubyError)}
                  onChange={(event) => {
                    setRubyText(event.target.value)
                    setRubyError(null)
                  }}
                />
                {rubyError && <FieldError>{rubyError}</FieldError>}
              </Field>
              <Field>
                <FieldLabel>{m.editor_placement()}</FieldLabel>
                <ToggleGroup
                  value={[rubyPosition]}
                  variant="outline"
                  spacing={0}
                  className="grid w-full grid-cols-2"
                  onValueChange={(values) => {
                    const value = values[0] as "over" | "under" | undefined
                    if (value) setRubyPosition(value)
                  }}
                >
                  <ToggleGroupItem value="over" className="w-full">
                    <SuperscriptIcon data-icon="inline-start" />
                    {m.editor_over_text()}
                  </ToggleGroupItem>
                  <ToggleGroupItem value="under" className="w-full">
                    <SubscriptIcon data-icon="inline-start" />
                    {m.editor_under_text()}
                  </ToggleGroupItem>
                </ToggleGroup>
              </Field>
            </FieldGroup>
            <DialogFooter>
              {editingRuby && (
                <Button type="button" variant="outline" onClick={removeRuby}>
                  {m.editor_remove_ruby()}
                </Button>
              )}
              <Button
                type="button"
                variant="outline"
                onClick={() => setRubyOpen(false)}
              >
                {m.common_cancel()}
              </Button>
              <Button type="submit" disabled={Boolean(rubyError)}>
                {editingRuby ? m.editor_update_ruby() : m.editor_add_ruby()}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}
