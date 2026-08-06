<!--
Thanks for contributing. Please read CONTRIBUTING.md if you have not yet —
it documents the layer boundaries this codebase enforces.
-->

## What and why

<!-- What changes, and what problem it solves. Link the issue: Closes #123 -->

## How it was verified

<!--
Which checks you ran, and what you exercised manually. Extraction and OCR
changes cannot be covered by CI — say which disc, provider, model, and how many
cues you tested against.
-->

## Checks

- [ ] `pnpm typecheck`
- [ ] `pnpm lint`
- [ ] `pnpm test` (Vitest + `cargo test --workspace`)
- [ ] `pnpm format`
- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`

## Boundaries

- [ ] No filesystem, SQLite, provider HTTP, or process launch added to the renderer
- [ ] New IPC methods and events are in **both** allowlists and the backend dispatcher
- [ ] Revisions remain append-only; OCR evidence is not mutated
- [ ] API keys do not reach any durable storage
- [ ] Sidecar stdout remains protocol-only (logging goes to stderr)

## Impact

- [ ] Changes the `.rosettacue` project schema (**invalidates existing projects** — explain below)
- [ ] Changes the sidecar protocol
- [ ] Changes the canonical JSON export format
- [ ] Updates `docs/specification.md` for changed documented behavior
- [ ] Adds a `CHANGELOG.md` entry under `Unreleased`

## Screenshots

<!-- UI changes: please include light and dark theme. -->
