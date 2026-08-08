# Changelog

All notable changes to RosettaCue are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

While RosettaCue is pre-1.0, minor version bumps may contain breaking changes to
the `.rosettacue` project schema, the sidecar protocol, and the exported JSON
document. Breaking changes are always listed first.

## [Unreleased]

### Changed — breaking

- **Project schema 2.** A Cue's recognized subtitle is now an ordered list of
  text blocks rather than a flat list of lines. Each block carries its own
  bounding box, writing direction (`horizontal_tb` or `vertical_rl`), nine-grid
  position, and provenance. The Cue edit document no longer has a position of
  its own: a Cue can hold blocks in different places, so placement belongs to
  the block. As before, there is no migration — a project package written by an
  earlier version is refused, and the track must be re-extracted.
- **Exported JSON document 2.** `cues[].position` is gone; placement is on each
  block inside `cues[].subtitle.blocks`. Export warnings are now objects with a
  stable `code`, the `cue_index`, and an English `message` for logs, instead of
  bare English sentences, and each kind of loss is reported once per Cue rather
  than once per line.
- **Prompt versions.** OCR prompts are `subtitle-ocr-v7` and translation
  prompts `subtitle-translation-v3`.

### Added

- **Layout analysis.** A new `rosettacue-layout` crate finds a Cue's text
  blocks, decides each one's writing direction, and estimates its rows or
  columns and characters, all from the bitmap's foreground mask. When the
  evidence is weak it degrades to one horizontal block covering the whole Cue,
  which recognizes exactly as it did before.
- **Vertical writing (縦書き).** Vertical blocks are cropped, described to the
  provider in their own direction, asked about ruby as right/left, and rendered
  through CSS `writing-mode` so the browser places furigana correctly.
- **Per-block editing.** A multi-block Cue shows a block tab strip with a
  direction toggle, per-block placement, and reading-order controls. The
  original bitmap gets block outlines drawn over it.
- **`rosettacue ocr layout-survey <project>`.** Reports a project's block-count
  distribution, writing-mode split, and analyzer doubts without contacting a
  provider. `--separation-em`, `--minimum-block-em2`, and `--maximum-blocks`
  try a threshold over a whole track before it is committed anywhere.
- **An OCR settings section.** The block-detection thresholds — separation in
  em, minimum fragment area in em², and the block-count cap — are now settings
  with a documented range, a stated default, and a restore-defaults action,
  rather than constants in the analyzer. The CLI's model config document takes
  the same three under a `layout` block. Every value is clamped inside the
  analyzer, so no caller can hand it a separation of zero.
- **Soft validation issues.** A transcription whose character count disagrees
  with the bitmap measurement is accepted and flagged for review rather than
  rejected. These are the first entries in the attempts table's `issues` column.
- **Export warnings are shown.** The export dialog lists them, localized from
  the warning code.

### Fixed

- **Vertical cues no longer fail recognition.** The row estimate projected the
  whole Cue horizontally and rejected any response that disagreed. A vertical
  column fragments into one band per glyph under that projection, so a correct
  answer was rejected and every retry consumed. Unit counting is now per block
  and counts columns for vertical blocks.
- **A line parted by ideographic spaces is one block again.** The em that scales
  the block-separation threshold was read from the median band extent along each
  axis. Most scripts break a glyph into several ink bands, so along the axis
  text flows that median measures a stroke rather than a glyph, and the
  threshold derived from it came out narrower than the blank an ideographic
  space leaves. `（鈴）ぶはあっ！　はあっ…　はあっ…` was cut into a block per
  phrase, each recognized as its own request and reassembled as separate blocks.
  The em now comes from the longest band on the axis that has fewer of them,
  which is the axis running across the text rather than along it.

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
