# ADR-0023: What an approval covers, untrusted folders, supply chain

- **Status:** accepted (owner, 2026-11-23)
- **Resolves:** `docs/decisions/proposed/CR-AUD-SEC-agent-gate-trust-supply-chain.md`
- **Amends:** `docs/protocols/agents.md` (session grants, principals, ACP permissions; no wire
  type changed), ADR-0015 (the gate), ADR-0016 (ACP), ADR-0021 (built-in servers in untrusted
  folders), `docs/architecture/security.md`

## Context

The comprehensive audit of 751ffc2 found approvals that went wider than what the person
approved: grants matched argv as a prefix and were never cleared or scoped; a self-declared MCP
name shared grants; an auto-allowed ACP `read` let the agent write; an approved command let the
next terminal run anything, with any `LD_PRELOAD`; `env.install` was approved blind. Opening a
cloned repository ran its `build.rs` through rust-analyzer. Bootstraps followed
`releases/latest`, and a checksum came from the artifact's own origin. The CR (AUD-008, 009,
010, 039, 040, 041, 103, 104, 105, 107, 109) was approved by the owner in the remediation; the
independent review's findings (B4, B6b, R4, N1, N2, N5, N7) refined it, and the owner asked for
every finding to be resolved, this ADR included (review N15).

## Decisions

1. **Session grants are exact.** A grant covers one (tool, effect, paths, argv, conditions) per
   workspace and principal, never a prefix. It is cleared on session delete, rewind, restore
   and connection close (lazily: on the next `agent/toolCall`), and is not offered for
   protected paths, opaque calls (MCP, managed installs) or target-less calls other than reads.
   An ACP terminal's environment is part of its conditions with every value compared whole (a
   long value by its SHA-256).
2. **Protected paths always ask.** A write under `.axiom/` (the workspace policy and providers),
   a `.git` directory at any depth (nested repositories, submodules), a bare repository's
   `<name>.git/`, `.husky/`, and what git itself reads for the workspace's repository (the
   effective hooks directory, the git and common directories, every configuration file and
   include, asked of git; if git cannot answer, config-like names) asks every time,
   whatever a policy rule, a grant or an ACP permission says (what is not covered: `security.md`
   §Agent permissions). The gate judges the path the file really has,
   through symlinks inside the workspace (`Workspace::canonical_path`), and the write refuses
   if the path no longer resolves there (`write_text_checked`).
3. **Principals are per connection.** `agent/toolCall` accepts `cli` / `mcp:<name>`; the daemon
   suffixes the connection (`#c<n>`), visible on the card and in the audit log.
4. **ACP permissions.** Only the person's approval of an edit covers the agent's writes, and the
   approved paths are compared literally (a file named `src/*` is not a glob). An approved
   execute covers the next terminal running the shown command; the terminal's env and cwd are
   on the card and asked for even under an argv rule; `outputByteLimit` is at most 16 MiB and
   non-string `args` are refused. A terminal belongs to the session that created it: its agent
   reaches it again after an idle reap and resume, and another session's agent is refused.
5. **Agent tools.** `build.run` gains `timeoutSecs` (default 600, at most 3600) and refuses with
   `conflict` when the task changed after approval. `env.install` runs only the same
   principal's plan, never a system plan, and its card lists the plan; `env.planInstall`
   without `version` never runs a program.
6. **Agent-started processes do not get the daemon's secrets**: variables whose names look
   secret (`_TOKEN`, `_KEY`, `_SECRET`, `_CREDENTIALS`, `_PAT`, `_AUTH`, `DATABASE_URL`,
   `PASSWORD`, `PASSWD`, `SSH_AUTH_SOCK`) and the model providers' `apiKeyEnv` variables, unless
   settings `agent.passEnv` names them; a withheld `SSH_AUTH_SOCK` is named first on the card,
   with how to pass it. External ACP agents and MCP servers keep the daemon's environment
   except the `apiKeyEnv` variables, and `env.install` programs get all of it. This is hygiene,
   not a boundary (`security.md` §Processes).
7. **Approval prompts are not held back by a stuck connection.** They go to every approving
   connection at once, while the gate already waits for the answer and the cancel; each send
   waits at most 10 s (`APPROVAL_DELIVERY`) for room on a full queue, much longer than the
   0.5 s other relays allow, because a prompt is for the person. A call fails
   `approval_required` only when the delivery reached no approver; an approver that attaches
   meanwhile is kept, and the resolution is never waited for on a connection that is not
   reading.
8. **Untrusted folders.** A built-in language server that would run the project's code runs
   restricted or not at all until the folder is trusted (`security.md` §Terminals and language
   servers). A refused one reports `state: "stopped"` with `detail` starting "Trust this folder
   to enable"; a restricted one starts "Restricted until you trust this folder".
9. **External agents and MCP servers run in their own process groups**, registered with the
   daemon's exit watchdog; stopping one kills its whole group (`nix` on Unix, as `exec` does).
10. **Supply chain.** System package names are validated (a leading `-` is refused); built-in
   bootstraps name a release; checksum-file digests are pinned on first use.

## Consequences

- More approval prompts: no prefix grants, protected paths, env-setting terminals, installs
  never remembered.
- Builds that read a token from the environment, and an agent's `git push` over SSH, need
  `agent.passEnv`.
- In an untrusted folder rust-analyzer, basedpyright, csharp-ls, dart, kotlin-lsp and
  sourcekit-lsp do not run until the person trusts the folder; the shell offers "Trust this
  folder".
- jdtls in restricted mode disables Gradle and Maven import; an Eclipse-format project with
  annotation processing enabled can still run its processor JARs (no jdtls setting turns that
  off; turning off autobuild would also drop whole-project diagnostics). Recorded in
  `security.md`.
- The protected paths are defence in depth under a policy that allows workspace writes, not a
  boundary: what an agent writes can be run later by the person's own tools or by programs git
  configuration names, in ways no list of names covers (`security.md` §Known gaps). **Owner's
  decision (2026-11-23): accepted as documented.** It is not taken up as isolation work now;
  ADR-0010's isolation tiers remain where it would be closed.
- **Open for the owner:** refusing jdtls entirely in untrusted folders (as other editors fall
  back to a syntax-only server there) would close jdtls's annotation-processing gap above, at
  the cost of Java language features until the folder is trusted.

## Alternatives rejected

- **Prefix or glob grants:** convenient, but they are how one approval came to cover commands
  and files the person never saw.
- **A new protocol field for server trust** (`ServerInfo.trust`): would let the shell avoid
  matching `detail` text; left for a later additive change.
- **Clearing grants eagerly on connection close:** the lazy clearing is safe (a reused `#c<n>`
  never inherits grants) and needs no new hook in the connection lifecycle.
