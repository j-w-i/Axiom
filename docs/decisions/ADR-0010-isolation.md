# ADR-0010: Isolation tiers and honest naming

Status: Accepted (2026-11-11); implementation from M3 (agents) and M6

## Context
The brief demands a clear distinction between managed installation and
genuine isolation. Research (`docs/research/toolchains.md` §4): shipping
agents use bubblewrap+seccomp (Codex, Anthropic sandbox-runtime) or Landlock
on Linux, Seatbelt (`sandbox-exec`, deprecated by Apple but in wide use) on
macOS, and restricted tokens/AppContainer-style mechanisms on Windows. Job
Objects and WSL2 are not security sandboxes by themselves.

## Decision
Three tiers with fixed names used in API, UI and docs:

| Tier | Meaning | Mechanism |
|------|---------|-----------|
| **Managed** | Where a toolchain is installed. No runtime restriction. | directories under `<data dir>/toolchains` |
| **Contained** | Process runs in a container. | Podman or Docker via the Dev Containers reference CLI or direct invocation; isolation = the runtime's |
| **Sandboxed** | Process runs under an OS policy Axiom configures: workspace read/write, toolchains read-only, no other writes, network off unless granted. | Linux: bubblewrap + seccomp (Landlock where bwrap is unavailable); macOS: Seatbelt profile; Windows: AppContainer / restricted token — each only after a startup self-test proves it enforces the policy |

Rules:
- An execution backend is an implementation of
  `trait ExecBackend { fn spawn(&self, spec, policy) -> … }` in `crates/exec`;
  `Host` (no isolation) is the M1 backend. Others are added behind the same
  trait.
- If the requested tier is unavailable on the host, the operation **fails
  with `unsupported`**; it never silently runs unisolated.
- The UI shows the tier of every running task. "Sandboxed" is never used for
  Managed or for Job-Object-only process grouping.
- Default for agent-initiated commands (M3): `Sandboxed` where available,
  otherwise `Host` with approval required for each command.
- Remote workers (M6) are a fourth backend: `Remote(ssh)`; its isolation is
  "another machine", stated as such.

## Consequences
- M1 ships with `Host` only and says so (`docs/architecture/security.md`
  §Known gaps).
- Per-OS sandbox work is substantial and is scheduled explicitly, not
  implied by the word "environment".
