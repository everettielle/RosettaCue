# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

RosettaCue is a local-first Electron desktop app that converts Blu-ray PGS image subtitles into structured, reviewable text using multimodal LLMs. It pairs a React/shadcn renderer with a Rust application core connected via an NDJSON sidecar protocol over stdin/stdout.

## Commands

```bash
corepack pnpm dev          # debug Rust sidecar + Vite dev server with Electron
corepack pnpm dev:web      # renderer only in a browser (no Electron bridge)
corepack pnpm typecheck    # TypeScript (runs i18n:compile first)
corepack pnpm lint         # ESLint for renderer + Electron
corepack pnpm test         # Vitest + cargo test --workspace
corepack pnpm format       # Prettier
corepack pnpm build        # release Rust + Electron/renderer bundles
corepack pnpm package      # electron-builder artifact for the host platform

cargo test --workspace                       # Rust tests only
cargo test -p rosettacue-ocr                 # single crate
cargo clippy --workspace --all-targets -- -D warnings  # Rust lint (pedantic, unsafe_code forbidden)
cargo fmt --all -- --check                   # Rust format check
cargo run -p rosettacue-cli -- doctor        # verify media tool resolution

corepack pnpm exec vitest run                # Vitest only
corepack pnpm i18n:compile                   # regenerate src/paraglide/ from messages/en.json
```

## Architecture

Three strict boundaries — crossing them is a design error:

1. **React renderer** — presentation and transient draft state. No filesystem, SQLite, provider HTTP, or process access. Uses `src/lib/desktop.ts` to call through the preload bridge.
2. **Electron main + preload** — native window, dialogs, sidecar lifecycle. No subtitle business rules or project SQL. Methods and events are allowlisted in `electron/contracts.ts`; both preload and main validate against these lists.
3. **Rust sidecar** — all project, subtitle, OCR, translation, and export behavior via `crates/core::Application`. The CLI (`apps/cli`) and Electron sidecar (`apps/backend`) are sibling adapters over the same `Application` API; neither may bypass core to touch project records.

### Rust crate graph

```
apps/backend, apps/cli → crates/core → crates/{domain, project, bluray, pgs, ocr, layout, llm, translation, export}
```

- `domain` — shared types (cue, subtitle, provider, geometry)
- `project` — `.rosettacue` package layout, SQLite schema v1, transactions, path confinement
- `bluray` — BDMV inspection and PGS demux via external media tools
- `pgs` — SUP segment parsing, RLE decode, PNG generation
- `layout` — foreground-mask analysis: block detection, writing direction, unit/glyph counting
- `llm` — provider protocol abstraction (LM Studio, Ollama, OpenAI, Anthropic)
- `ocr` — OCR pipeline, prompting, schema validation, language-specific normalization
- `translation` — structured subtitle translation
- `export` — canonical JSON and derived SRT assembly
- `diagnostics` — structured diagnostic event emission

### Renderer stack

React 19, TypeScript 6, Vite 8, Tailwind CSS 4, shadcn/ui preset `b27GcrRo` (Base UI primitives — **not Radix**), Inter Variable, Lucide icons. Paraglide JS for i18n (English only; source catalog in `messages/en.json`, generated runtime in `src/paraglide/` which is gitignored).

### IPC protocol

Electron ↔ Rust communicate via NDJSON on stdin/stdout. Request IDs allow concurrent out-of-order completion. A single Rust writer thread serializes all output. Stdout is protocol-only — diagnostics go to stderr.

Adding an IPC method or event requires updating: `electron/contracts.ts` (allowlists), `apps/backend/src/main.rs` (dispatcher), `src/types/desktop.ts` (renderer types), and the catalog in `docs/specification.md`.

## Key invariants

- Revisions are **append-only**. Editing, restoring, translating all append new revisions; nothing rewrites earlier ones or mutates OCR evidence.
- Any new revision resets the cue to `unreviewed`. Approval is bound to the exact revision it approved.
- API keys never reach durable storage (project DB, logs, exports, localStorage).
- Sidecar stdout is protocol-only JSON. Free-form logging goes to stderr.
- Pause/stop take effect only at safe cue transaction boundaries.
- Styled spans must compose exactly back to `OcrLine.text`; adjacent text spans with identical styles are maximally coalesced; ruby boundaries never merge.
- Project schema version is 1 with no migration code. Bumping it invalidates all existing packages.

## Renderer conventions

- Compose existing shadcn components; add new ones via `corepack pnpm dlx shadcn@latest add <component>`.
- Use Base UI's `render` prop, not Radix's `asChild`.
- Use semantic theme tokens (`background`, `foreground`, `muted`, `card`, `border`). Never hard-code light/dark pairs.
- Conditional class names go through `cn()`.
- Icon-only controls need tooltip + `aria-label`.

## Rust conventions

- Clippy at `pedantic` level, `unsafe_code` forbidden workspace-wide.
- Edition 2024, minimum Rust 1.97.1.
- Tests live in `#[cfg(test)]` modules next to the code.

## Commit style

Conventional Commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`. Scope with area when helpful: `fix(ocr):`, `feat(export):`.

## Project format

`.rosettacue` is a directory package containing `project.sqlite` and `assets/`. The Rust `project` crate owns the schema; Electron never reads SQLite directly.
