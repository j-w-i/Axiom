# Fixtures

All of them are **from documentation**; nothing here was recorded. No harness was run, and no account was used. What the agent says
(`say`) has the shapes of the pages named below, read on 2026-11-25. Where a page prints an example it is that one, with identifiers made the
same across messages and fields a message must have added from the schema; where it only describes a message, the values in it (a command, a
file's name, a few words of text) are ours. Messages from several pages were put in one order. What Axiom says (`hear`) is what the adapter must send. Each file is a script for `tests/fake/main.rs`, which plays the agent in a process of its own.

| File | From |
|------|------|
| `acp-v1-from-documentation.jsonl` | <https://agentclientprotocol.com/protocol/v1/initialization>, `/session-setup`, `/session-modes`, `/prompt-turn`, `/tool-calls` (by way of <https://agentclientprotocol.com/llms-full.txt>); schema `schema/v1/schema.json` of `agentclientprotocol/agent-client-protocol`, branch `main` |
| `acp-v2-from-documentation.jsonl` | <https://agentclientprotocol.com/protocol/v2/initialization>, `/session-setup`, `/session-config-options`, `/prompt-lifecycle`, `/tool-calls`, `/agent-plan`, `/migration`; schema `schema/v2/schema.json` there. Version 2 is a preview |
| `codex-from-documentation.jsonl` | <https://learn.chatgpt.com/docs/app-server.md> (the same text as <https://developers.openai.com/codex/app-server.md>), <https://learn.chatgpt.com/docs/agent-approvals-security.md>; schema `codex-rs/app-server-protocol/schema/json` of `openai/codex`, branch `main` |
| `claude-from-documentation.jsonl` | <https://code.claude.com/docs/en/headless.md>, `/cli-reference.md`, `/permission-modes.md`, `/sandboxing.md`, `/agent-sdk/typescript.md` (messages, `PermissionResult`, the `control_request` envelope, `SDKControlInitializeResponse`, `SDKControlInterruptResponse`), `/agent-sdk/streaming-output.md`, `/agent-sdk/user-input.md` |

Claude Code's documentation names the envelope `{"type": "control_request", "request_id", "request"}` and says that a client may drive the
control protocol directly. It does **not** give the `subtype`s of requests, the fields of `can_use_tool`, or the value `stdio` of
`--permission-prompt-tool` (which it describes for an MCP tool only). Those are from the source of the vendor's published SDK
(`anthropics/claude-agent-sdk-python`, MIT, branch `main`: `types.py`, `_internal/query.py`, `client.py`), as
`docs/research/harness-integration.md` §3 has them. The adapter passes that flag only when it is told to (`Prompts::Stdio`).
