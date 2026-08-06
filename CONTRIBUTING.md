# Contributing to RosettaCue

Thanks for considering a contribution. RosettaCue is a small, opinionated
codebase with strict layer boundaries — this document explains what those
boundaries are so your patch lands cleanly.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

- **Bugs:** open an issue with the reproduction, your platform, and which
  provider/model you were using. Attach the OCR attempt's raw response when the
  problem is recognition quality — it is stored in the project package.
- **Features:** open an issue first. RosettaCue has an explicit
  [out-of-scope list](docs/specification.md#12-out-of-scope) (playback, disc
  decryption, Aegisub-style authoring, cloud sync), and a large PR against one of
  those will be declined no matter how good it is.
- **Security:** do not open an issue. See [SECURITY.md](SECURITY.md).

## Setup

```bash
corepack enable
corepack pnpm install
corepack pnpm dev
```

Requirements are listed in the [README](README.md#requirements): Node.js 24+,
pnpm 11.20.0, Rust 1.97.1+, and the `bd_list_titles`, `bd_splice`, and `ffmpeg`
media tools on your `PATH` or staged in `resources/tools/`. Verify tool
resolution with `cargo run -p rosettacue-cli -- doctor`.

### Agent skills

The repository pins a set of agent skills in `skills-lock.json` (currently the
shadcn/ui `shadcn` and `migrate-radix-to-base` skills). The skill files
themselves are **not committed** — `.agents/` is gitignored. Restore them with:

```bash
corepack pnpm dlx skills experimental_install
```

They are optional, but if you work on the renderer with an AI coding agent they
carry the shadcn/Base UI conventions this project follows. Add a skill with
`pnpm dlx skills add <owner>/<repo>` and commit the resulting `skills-lock.json`
change only.

## Checks

Every PR must pass all four:

```bash
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm test        # Vitest + cargo test --workspace
corepack pnpm format      # Prettier; run it, don't just check it
```

CI runs these on macOS, Windows, and Linux. Rust code must be `cargo fmt`-clean
and `cargo clippy --workspace --all-targets -- -D warnings` clean; the workspace
enables `clippy::pedantic` and forbids `unsafe_code`.

## Architecture rules

These are not style preferences. A PR that crosses them will be sent back.

### Layer boundaries

| Layer | Owns | Must never touch |
| --- | --- | --- |
| React renderer | Layout, dialogs, selection, filters, unsaved cue drafts, progress display | Filesystem, SQLite, provider HTTP, process launch |
| Preload | Narrow `invoke`, event subscription, dialogs, window-mode facade | Raw `ipcRenderer`, non-allowlisted channels |
| Electron main | Window, dialogs, external-link policy, sidecar lifecycle | Subtitle business rules, project SQL |
| Rust sidecar | NDJSON parsing, typed command decoding, progress emission | Visual state, native window behavior |
| `crates/core` | Use-case orchestration and domain validation | Any Electron or React dependency |
| `crates/project` | Package layout, schema, transactions, path confinement | Network calls, UI |
| Media crates | Inspection, demux, SUP decode, PNG generation | Project workflow policy |
| LLM/OCR crates | Provider protocol, prompting, validation, normalization | API-key persistence, project UI |

The CLI and the Electron sidecar are sibling adapters over the same
`Application` API. Neither may bypass the core to manipulate project records.
If you find yourself adding SQL to `electron/` or a `BrowserWindow` reference to
a crate, the design is wrong.

### Adding an IPC method or event

A new method or event must be added to **both** allowlists — `electron/contracts.ts`
feeds the main-process and preload validators — and to the corresponding
dispatcher arm in `apps/backend/src/main.rs`. Missing either side fails closed,
which is intended. Update the command or event catalog in
[docs/specification.md](docs/specification.md#5-ipc-and-sidecar-protocol) in the
same PR.

### Data invariants

The [specification's invariant list](docs/specification.md#17-system-invariants)
is the contract. The ones most easily broken by accident:

- Revisions are **append-only**. Editing, restoring, and translating all append;
  nothing rewrites an earlier revision or mutates OCR evidence.
- Any new revision resets the cue to `unreviewed`. Approval is bound to the exact
  revision it approved.
- Styled spans must compose exactly back to `OcrLine.text`, every span carries a
  canonical style array, and adjacent text spans with identical styles are
  maximally coalesced — but ruby boundaries never merge.
- API keys never reach durable storage of any kind.
- Sidecar stdout is protocol-only. Logging goes to stderr, always.
- Pause and stop take effect only at safe cue transaction boundaries.

### Project schema

The current schema version is 1 and RosettaCue ships no migration code by
design. If your change needs a schema change, bump `PROJECT_SCHEMA_VERSION`, update
the schema contract in the specification, and say so loudly in the PR — it
invalidates every existing project package.

## Renderer conventions

The UI is shadcn/ui preset `b27GcrRo`: `base-rhea` style, **Base UI** primitives
(not Radix), neutral base color, Inter Variable, Lucide icons, Tailwind CSS 4.

- Compose installed shadcn components before writing a custom primitive. Check
  what exists with `corepack pnpm dlx shadcn@latest info`, and add components
  through the CLI: `corepack pnpm dlx shadcn@latest add <component>`.
- Use `FieldGroup` and `Field` for forms.
- Use Base UI's `render` prop, **not** Radix's `asChild`.
- Use semantic theme tokens (`background`, `foreground`, `muted`, `card`,
  `border`). Never hard-code a light/dark pair in a component.
- Route conditional class names through `cn()`.
- Lucide icons inside buttons carry `data-icon`.
- Keep custom classes focused on application layout.

Accessibility is a requirement, not a nice-to-have: commands need accessible
names, icon-only controls need a tooltip and `aria-label`, inputs need labels,
and status must never be conveyed by color alone.

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/) —
  `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`. Scope with the area
  when it helps: `fix(ocr): ...`, `feat(export): ...`.
- One logical change per PR. Keep formatting churn out of behavioral changes.
- Add tests. Rust logic goes in `#[cfg(test)]` modules next to the code;
  renderer logic goes in a `*.test.ts` beside the module. Pure functions —
  span normalization, cue geometry, project-name handling, JA normalization —
  are where the value is; that is where the existing tests live.
- Update `docs/specification.md` when you change a documented behavior, and add
  a `CHANGELOG.md` entry under `Unreleased`.
- Describe how you verified the change. For UI work, include a screenshot in
  both light and dark themes.

## Testing notes

Some behavior needs a real disc backup and a real model, which CI cannot
provide. When you touch extraction or OCR, state in the PR what you tested
against: which disc, which provider, which model, and how many cues.

Do not run a development build and a packaged build at the same time — they
share a product name and bundle identifier and will confuse each other's state.

## What we will not merge

- Disc decryption, AACS/BD+ handling, or any copy-protection circumvention.
- Third-party binaries committed to the repository.
- Telemetry, analytics, crash reporting, or any implicit network call.
- Anything that gives the renderer filesystem or process access.
- Project-format migration shims, until the format is declared stable.
