# Security Policy

## Supported versions

RosettaCue is pre-1.0. Only the latest commit on `main` receives security fixes.

| Version | Supported |
| --- | --- |
| `main` | ✅ |
| Tagged pre-releases | ❌ |

## Reporting a vulnerability

**Please do not open a public issue for a security vulnerability.**

Report it privately through
[GitHub Security Advisories](https://github.com/everettielle/RosettaCue/security/advisories/new).
That channel is private between you and the maintainer, and it is the only
supported way to report a vulnerability.

Please include:

- what an attacker can do, and what they need in order to do it;
- affected component (renderer, preload, Electron main, sidecar protocol, a Rust
  crate, or the packaged build);
- reproduction steps or a proof of concept;
- the commit or release you tested.

You can expect an acknowledgement within 7 days. RosettaCue is a personal
project with no paid maintainers and offers no bug bounty, but credit in the
advisory and changelog is offered for any valid report you want it for.

Please give a reasonable window for a fix before public disclosure — 90 days is
the default, shorter if the issue is already being exploited.

## Scope

RosettaCue is a local-first desktop application with no server component. The
security properties that matter most, and where reports are most valuable:

- **Renderer isolation.** The renderer runs with `contextIsolation: true`,
  `nodeIntegration: false`, `sandbox: true`, and `webSecurity: true`. Any path
  by which renderer content reaches Node.js, the filesystem, process spawning,
  or raw `ipcRenderer` is in scope.
- **IPC allowlists.** Electron main and the preload bridge each validate method
  and event names against static allowlists. A bypass is in scope.
- **Path confinement.** Cue image reads resolve project-relative paths and must
  reject traversal outside the `.rosettacue` package. Save As must reject
  destinations inside the source package. Escapes are in scope.
- **Credential handling.** API keys are session-only renderer state. They must
  never reach the project package, OCR run settings, local preferences, logs,
  error messages, or exports. Any leak is in scope.
- **Protocol integrity.** The sidecar's standard output must contain protocol
  JSON only. Injection into that stream, or a malformed-input crash in the
  NDJSON dispatcher, is in scope.
- **Untrusted input parsing.** The PGS decoder and Blu-ray inspection code parse
  attacker-influenced binary data. Memory-safety issues, panics reachable from a
  crafted `.sup` or BDMV structure, and decompression bombs are in scope.
  (`unsafe_code` is forbidden workspace-wide, so panics and resource exhaustion
  are the realistic failure modes.)

### Out of scope

- Vulnerabilities in third-party media tools (libbluray, FFmpeg) — report those
  upstream.
- Vulnerabilities in LLM providers you configure.
- Model output quality, hallucinated recognition, or bad translations.
- Attacks requiring an already-compromised local user account.
- Anything requiring the user to deliberately open a malicious project package
  they were handed — though we will still take path-traversal reports seriously,
  since project packages are a plausible sharing unit.
