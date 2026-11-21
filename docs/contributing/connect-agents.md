# Connecting external agents to Axiom (MCP)

`axiom mcp` is an MCP server on stdio. It gives an MCP client (Claude Code, Codex, Gemini CLI,
or any other) Axiom's tools for one workspace: `workspace.search`, `fs.read`, `fs.applyPatch`,
`build.run`, `task.run`, `env.*` and the rest. Behind it:
- **Axiom's permission policy decides every call.** A call that needs approval appears in the
  Axiom window, if one is open on the same data dir. Otherwise it is refused, with the reason
  as the tool result.
- **Writes are checkpointed** (under `ext-mcp_<client>`) and every call is in the audit log.

Build it with `cargo build -p axiom-cli -p axiomd` (binaries in `target/debug/`), or use the
installed `axiom`. Run it from the project folder, or pass `--workspace <folder>`.

## Claude Code
Add to the project's `.mcp.json`:
```json
{ "mcpServers": { "axiom": { "command": "axiom", "args": ["mcp"] } } }
```
or run `claude mcp add axiom -- axiom mcp` in the project folder.

## Codex
Add to `~/.codex/config.toml`:
```toml
[mcp_servers.axiom]
command = "axiom"
args = ["mcp", "--workspace", "/path/to/project"]
```

## Notes
- **Desktop app open:** `axiom mcp` uses the app's daemon (through its socket), so approvals
  show in the app.
- **Desktop app closed:** `axiom mcp` starts its own daemon. Nothing can approve then, so set
  the policy (Settings, or `agent/setPolicy`) to allow what you want the agent to do.
- **Client config formats:** the formats above follow each client's documentation at the time
  of writing. M3-MCP-001 tested `axiom mcp` with an in-repo MCP client, not with Claude Code or
  Codex themselves.
- **Hosting inside Axiom instead:** to run Claude Code or Codex in Axiom's own chat, see
  ADR-0016 (ACP).
