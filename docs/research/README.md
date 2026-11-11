# Research

Technology research carried out on 2026-11-11 by research agents with web
access, each restricted to primary sources and required to cite them inline
and to list what they could not verify. These files are **evidence, not
decisions**: decisions are in `../decisions/`, and where an ADR deviates
from a research recommendation it says so (e.g. ADR-0003 on framing).

| File | Topic | Feeds |
|------|-------|-------|
| `qt-rust.md` | Qt 6 versions and licensing, CXX-Qt and alternatives, IPC options, packaging, CI, GUI testing | ADR-0001, 0002, 0003, 0011 |
| `editor.md` | Monaco, monaco-vscode-api, Code-OSS, Theia, Open VSX, marketplace terms, LSP/DAP, WebEngine hosting | ADR-0004 |
| `toolchains.md` | Version managers, Nix, Dev Containers, OS sandboxes, package managers, structured build output, remote execution | ADR-0006, 0010 |
| `xtool.md` | xtool capabilities, prerequisites, legal notes, provider adapter outline | M6 epic 6.2 |
| `visual-inspection.md` | CDP, React/Vue/Svelte source mapping, Qt/GammaRay, Flutter, Android, Apple, accessibility, visual grounding, prior art | ADR-0008 |
| `agents.md` | Model APIs, MCP, ACP, existing harnesses, tool design, permissions, checkpoints, sessions | ADR-0007 |
| `prior-art.md` | Other IDEs' architectures, plugin isolation models, Rust building-block crates | ADR-0001, 0006 |

Each file ends with "Unverified / needs experiment". Items that affect the
architecture are consolidated in `../architecture/open-questions.md`.
Local experiments that settled questions empirically: `../../experiments/README.md`.
