# ADR-0004: Editor — Monaco first, language servers in the daemon, extension host later

Status: Accepted (2026-11-11)

## Context
"VS Code-based editing with compatible extensions" spans a wide range of
possible meanings and costs. Facts established by `docs/research/editor.md`:
- Monaco is the editor widget only: no extension host, no workbench, no
  TextMate grammars by default.
- `@codingame/monaco-vscode-api` (MIT) layers real VS Code services onto
  Monaco and can run **web** extensions in a worker; **Node** extensions need
  a separate, version-matched VS Code server process.
- The Microsoft Marketplace may not be used by non-Microsoft products; Open
  VSX is the usable registry. Pylance, cpptools, C# Dev Kit and Remote-* are
  licensed for Microsoft products only.
- Embedding the full Code-OSS workbench gives the highest compatibility but
  brings a second, competing application UI inside ours.

## Decision
Staged, with an honest compatibility ceiling stated at each stage:

| Stage | Milestone | What | Ceiling |
|-------|-----------|------|---------|
| 1 | M1 | Plain Monaco (pinned), bundled offline, served from `axiom://editor/`, bridged by QWebChannel | Syntax colouring (Monarch), Monaco's built-in TS/JS/JSON/CSS/HTML services. No extensions. |
| 2 | M4 | Language servers supervised by the daemon (`crates/lsp`); the editor surface registers Monaco providers that call the daemon through an LSP relay channel | Whatever each language server offers (rust-analyzer, clangd, basedpyright, typescript-language-server, sourcekit-lsp, qmlls). No extension APIs. |
| 3 | M7 spike | `monaco-vscode-api` service overrides: TextMate grammars, themes, **web** extensions from Open VSX | Declarative and web-worker extensions only. |
| 4 | M7+ decision | Node extension host via a version-matched Code-OSS server | Most open-source extensions; never Microsoft-proprietary ones. Requires its own ADR after the stage 3 spike. |

Compatibility is only ever claimed for extensions that pass the automated
compatibility suite (introduced with stage 3).

The **bridge contract** (`docs/protocols/editor-bridge.md`) is the stable
seam: the shell knows documents, saves and cursor positions, not Monaco. A
later stage can replace what is behind the bridge without touching the shell
or daemon.

### Language intelligence (stage 2 outline)
LSP servers are child processes of the daemon, one per (workspace, server).
The daemon owns their lifecycle, performs `initialize`, and relays LSP
messages to the editor surface over a dedicated channel
(`lsp/send`, `lsp/receive` notifications tagged with a server id). The editor
surface holds the LSP client logic (document sync from Monaco models,
provider registration) because it owns the unsaved text. Agents reach the
same servers through `lsp.*` tools served by the daemon. Detailed design is
the first task of M4.

## Why not start at stage 3 or embed the workbench now
- M1 needs an editor that opens, edits and saves. Monaco does that today (E1)
  with ~14 MB of assets and no backend.
- `monaco-vscode-api` tracks VS Code monthly and must be version-aligned
  with its companion packages; adopting it before we have an editor test
  suite would put a moving dependency under the foundation.
- The workbench would duplicate our explorer, panels and AI surfaces, and
  hide the document model from our agent and inspection features.

## Consequences
- Until M4, non-web languages get colouring only. This is stated in the UI
  nowhere as a feature; the roadmap is explicit.
- Qt WebEngine concerns to keep testing on every OS: keyboard routing (E1:
  works on Linux), IME, clipboard, drag and drop, accessibility. These are
  M1 review checklist items, not assumptions.
