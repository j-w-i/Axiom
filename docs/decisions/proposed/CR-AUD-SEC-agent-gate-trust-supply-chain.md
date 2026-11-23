# CR-AUD-SEC: what an approval covers, untrusted folders, supply chain

_Status: **Resolved — accepted by ADR-0023 (2026-11-23).** Approved by the owner (2026-11-23, remediation of the comprehensive audit: "fix
every issue"). Raised by AUD-008, AUD-009, AUD-010, AUD-039, AUD-040, AUD-041, AUD-103,
AUD-104, AUD-105, AUD-107, AUD-109 (`audits/comprehensive-audit/ISSUES.md`). Implemented in
`crates/{agent,env,lsp,inspect}`, `crates/axiomd/src/{tools,acp}.rs`, the agent, session and
LSP handlers, `providers/{python,rust}`; behaviour in `docs/architecture/security.md`._

## Original assumption

The frozen agent-tool contract (`agents.md`) said an "allow for session" grant covers "that
tool and target set", that `agent/toolCall` principals are `mcp:<name>` and `cli`, and that an
ACP permission covers "the next command". The provider contract allowed bootstraps from
`releases/latest`. Language servers started in any folder (`env/trust` gated only workspace
providers).

## Evidence against it

The audit showed each of these goes wider than what the person approved: grants matched argv
as a prefix and were never cleared or scoped; a self-declared MCP name shared grants; an
auto-allowed `read` permission let the agent write the file; an approved command let the next
terminal run anything, with any `LD_PRELOAD`; `env.install` was approved blind; `build.run`
ran a re-resolved command. Opening a cloned repository ran its `build.rs` through
rust-analyzer. A release published between a checksum fetch and a download broke installs,
and the checksum came from the artifact's own origin.

## Change (no wire type changed; additive where the contract is touched)

- **Agent tools.** `build.run` gains the optional input `timeoutSecs` (default 600, at most
  3600). Session grants are exact (tool, effect, paths, argv, conditions) per workspace and
  principal, cleared on delete/rewind/restore and connection close, and not offered for
  protected paths, opaque calls or target-less calls other than reads. Writes under `.axiom/`
  and `.git/` always ask. `acp.terminal` puts the agent's env and cwd on the card and asks for
  them even under an argv rule; `outputByteLimit` ≤ 16 MiB; non-string args refused.
  `env.install` runs only the same principal's plan, never a system plan, and its card lists
  the plan. `env.planInstall` without `version` never runs a program. `build.run` refuses with
  `conflict` when the task changed after approval. Agent-started processes do not get
  secret-looking variables or `apiKeyEnv` variables (settings `agent.passEnv`, a new free-form
  settings key, re-allows by name).
- **Principals.** `agent/toolCall` still accepts `cli` / `mcp:<name>`; the daemon suffixes the
  connection (`#c<n>`), visible in `ApprovalRequest.principal` and the audit log.
- **ACP permissions.** Only a person's approval of an edit covers the writes, and of an execute
  the next terminal running the shown command.
- **Language servers (no protocol change).** In an untrusted folder a built-in server runs as
  configured, restricted, or not at all (`security.md` §Terminals and language servers). A
  refused one reports `state: "stopped"` with `detail` starting "Trust this folder to enable";
  a restricted one runs with `detail` starting "Restricted until you trust this folder". A
  future additive protocol field (for example `ServerInfo.trust: "restricted" | "needsTrust"`)
  would let the shell avoid matching text; it is not added here.
- **Provider manifests (no schema change).** System package names are validated (a name
  starting with `-` was accepted before); built-in bootstraps name a release; checksum-file
  digests are pinned on first use.

## Consequences

More approval prompts (no prefix grants; protected paths; env-setting terminals; installs never
remembered). Some builds that read a token from the environment need `agent.passEnv`. In an
untrusted folder rust-analyzer, basedpyright, csharp-ls, dart, kotlin-lsp and sourcekit-lsp do
not run until the person trusts the folder; the shell should offer "Trust this folder".
