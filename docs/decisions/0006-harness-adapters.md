# 0006 Delegation: ACP first, native where ACP loses something
**Decision.** Order: ACP client (v1, v2 negotiated), Codex app-server, Claude stream-json, Copilot SDK. Adapters declare capabilities; the client hides what is absent; nothing is emulated.
Raw frames are journalled. Axiom gates trust **before** spawn and owns the sandbox when it can (SI-26).
**Evidence.** `research/harness-integration.md`: 46 agents in the ACP registry; ACP lacks quota events, stable fork and a pre-tool veto. Cursor's SDK auto-approves headless; `claude -p` runs repo hooks without trust.
**Consequence for the brief.** Gemini CLI's consumer tier ended 2026-06-18. M3 targets Antigravity (`agy acp`) for Google plans, marked `interactiveOnly`, and Gemini CLI for API-key and enterprise users.
**Alternatives.** Terminal scraping (kept only as a labelled fallback); one SDK per vendor (weight, three runtimes).
**Revisit.** When ACP v2 leaves preview.
