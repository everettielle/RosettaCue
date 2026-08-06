# Changelog

All notable changes to RosettaCue are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

While RosettaCue is pre-1.0, minor version bumps may contain breaking changes to
the `.rosettacue` project schema, the sidecar protocol, and the exported JSON
document. Breaking changes are always listed first.

## [Unreleased]

Nothing yet.

## [0.1.0] — 2026-08-06

First public release.

### Added

- **Desktop shell.** Electron 43 with a single `BrowserWindow` in two modes: a
  fixed 960×600 Welcome chooser and a resizable Workspace editor. Renderer runs
  with `contextIsolation`, `sandbox`, and `webSecurity` on and `nodeIntegration`
  off, reaching Rust only through allowlisted preload methods and events.
- **Sidecar protocol.** NDJSON request/response/event framing over stdin/stdout
  with request-ID correlation and a single serializing writer thread.
- **Projects.** Create, open, removable recent items, missing-project recovery,
  and Save As cloning through a hidden temporary directory and atomic rename.
- **Blu-ray source handling.** Read-only BDMV inspection via `bd_list_titles`,
  PGS demux via `bd_splice` and `ffmpeg`, and SUP decoding to timed PNG cues
  published incrementally during extraction.
- **OCR pipeline.** Three passes — main text, ruby annotations, italic ranges —
  each schema-validated with bounded corrective retries. A foreground-pixel row
  projection estimates large glyph rows before the first request and rejects
  responses whose line count disagrees. Japanese normalization records every
  change it makes.
- **OCR job control.** Pause, resume, and stop that take effect only at safe cue
  transaction boundaries, with durable per-cue checkpoints in the `jobs` table.
  Stale `running`/`paused` jobs become `interrupted` on restart and can be
  resumed with a fresh provider profile.
- **Editor.** Selection-based bold, italic, underline, strikethrough,
  superscript, and subscript; ruby creation, editing, removal, and over/under
  placement; canonical span normalization that recoalesces adjacent identical
  runs while preserving ruby boundaries; timing editing and a 3×3 position grid;
  explicit Save Cue.
- **Preview.** Source bitmap and structured subtitle rendered side by side in one
  shared Blu-ray canvas coordinate system, so position and scale are comparable.
- **Review.** Append-only revision history, revision-based undo/redo, explicit
  approval, and automatic invalidation when a new revision arrives.
- **Translation.** Per-cue and whole-track structured translation preserving
  timing and semantic position.
- **Providers.** LM Studio, Ollama, OpenAI, and Anthropic, configured as
  independent OCR, validation, and translation profiles, with reachability and
  latency diagnostics. API keys stay in renderer session memory only.
- **Export.** Canonical JSON with geometry, ruby, per-span styles, review status,
  and OCR provenance; derived SRT with portable `<b>`/`<i>`/`<u>` markup and
  explicit warnings for flattened ruby and unsupported styles.
- **CLI.** `apps/cli`, a sibling adapter over the same application core, covering
  doctor, project, source, ocr, translate, and export.
- **Packaging.** electron-builder configuration for macOS (DMG, ZIP), Windows
  (NSIS), and Linux (AppImage, DEB), staging the native sidecar and media tools
  into Electron resources.

[Unreleased]: https://github.com/everettielle/RosettaCue/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/everettielle/RosettaCue/releases/tag/v0.1.0
