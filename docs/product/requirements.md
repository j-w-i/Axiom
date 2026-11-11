# Product requirements

Axiom is an open-source, cross-platform (Linux, macOS, Windows) development
platform designed around LLM agents from the start. The original brief is
long; this file is its distilled, numbered form. Requirement ids are stable
and are referenced from task specs.

## Principle

Build a platform that **understands and orchestrates** development
environments. The GUI, the agent runtime, the CLI and tests are all clients of
one service layer. Nothing a user can do in the GUI is implemented only in the
GUI.

## Functional requirements

| Id | Requirement | Milestone |
|----|-------------|-----------|
| R1 | Open a directory as a workspace; browse, open, edit and save files | M1 |
| R2 | Run a project command; see structured output, errors and exit status | M1 |
| R3 | Persist settings and per-workspace UI state across restarts | M1 |
| R4 | Detect which toolchains a project needs and which are installed | M2 |
| R5 | Install toolchains either into Axiom-managed storage or via the system package manager, with explicit consent, version selection and clean removal | M2 |
| R6 | Build/run/test through existing build systems; diagnostics are structured and navigable | M2 |
| R7 | Connect local and remote LLMs through a vendor-neutral provider interface | M3 |
| R8 | Agents act through typed tools on the same services the GUI uses, under a permission policy, with recoverable checkpoints | M3 |
| R9 | Sessions, tasks and context persist across independent agent sessions | M3 |
| R10 | Language intelligence (completion, diagnostics, navigation, rename, format) through language servers | M4 |
| R11 | Integrated terminal and Git status/diff | M4 |
| R12 | Preview a running web app inside Axiom; select an element; resolve it to source | M5 |
| R13 | Closed loop: agent patches source, app reloads, result is verified by geometry, pixels and diagnostics, not by "it compiled" | M5 |
| R14 | A second, non-web inspection adapter (Qt/QML) proves the protocol is framework-independent | M6 |
| R15 | Third parties add toolchain providers and inspection adapters without modifying core | M2 (providers), M6 (adapters) |
| R16 | Apple development on non-Apple hosts through xtool, within what xtool actually supports | M6 |
| R17 | Debugging through the Debug Adapter Protocol | M6 |
| R18 | Compatible VS Code extensions, with compatibility demonstrated by an automated suite before it is claimed | M7 |
| R19 | Installers and packages for all three OSes | M7 |

## Non-functional requirements

| Id | Requirement |
|----|-------------|
| N1 | No OS assumptions in service code; platform differences sit behind small, named abstractions |
| N2 | The GUI thread never blocks on the backend |
| N3 | A crash of the editor surface, a provider, a language server or an agent does not take down the daemon; a daemon crash does not lose unsaved editor buffers |
| N4 | Nothing is called "sandboxed" unless it is genuinely isolated; managed installation and isolation are different things and are labelled differently in UI and API |
| N5 | Sensitive operations (writes outside the workspace, installs, network, credentials, deletes, deploys) are never executed silently |
| N6 | Every visible control does something. Unfinished features are absent, not stubbed |
| N7 | Light and dark themes, keyboard access to every action, visible focus |
| N8 | The repository alone (no chat history) is enough for a new contributor or agent to continue |

## Explicit non-goals

- Reimplementing the VS Code extension API. We reuse existing implementations
  or do without (ADR-0004).
- Replacing build systems. We orchestrate Cargo, CMake, Gradle, SwiftPM, etc.
- Promising source mapping for frameworks that expose no metadata (SwiftUI on
  device, arbitrary release binaries). Those get Level 1 or 2 only
  (`docs/protocols/inspection.md`).
- Access to the Microsoft extension marketplace or Microsoft-licensed
  extensions (Pylance, cpptools, C# Dev Kit). See `docs/research/editor.md`.

## Success scenario (north star)

Install Axiom → open a project → Axiom identifies and installs its toolchain →
edit → build and run → preview the UI → click a misplaced button → ask the
agent to fix it → agent resolves the element to source, proposes a patch →
user reviews → patch applied → app reloads → Axiom verifies the button moved
as intended and nothing else changed → continue next week with context intact.
