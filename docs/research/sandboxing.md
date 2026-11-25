# Sandboxing, permission policy and code-mode runtimes

Status: research, 2026-11-24. **[S]** source read (shallow clones: codex `a0d5b0a`, sandbox-runtime `0bbebed` v0.0.83,
gemini-cli `03862c4`, monty `bdf4f6e`), **[D]** vendor doc, **[A]** advisory, **[T]** tested here on macOS 27.0 arm64,
**[I]** inference. Nothing was tested on Linux or Windows.

## 1. Conclusions

1. **One Rust trait, three native backends plus an optional VM backend.** macOS Seatbelt via `/usr/bin/sandbox-exec`; Linux
   bubblewrap + seccomp (+ Landlock); Windows **MXC BaseContainer/PSEC where present, otherwise WSL2, otherwise no
   auto-run**. Fail closed everywhere (SI-8).
2. **Nested Seatbelt fails on macOS 27.0** [T]. Wrapping Claude Code or Codex in our profile breaks their inner sandbox.
   Per session choose outer (ours, harness sandbox off) or inner (harness's). Linux nests if we allow user namespaces.
3. **Windows is the schedule risk.** Anthropic's documented Claude Code position is that native Windows commands run
   unsandboxed [D]; Cursor uses WSL2 [D]; Codex maintains ~150 Rust files: WFP filters, sandbox accounts, a service [S].
   Microsoft's MXC (MIT, 1.0 on 2026-10-07) is the first plausible shared answer, but its SDK sends **Required telemetry
   that hosts cannot disable** in routed release builds [D]. Building the crate without routing is possible; needs an ADR.
4. **The proxy is the policy point for network, the kernel is the boundary.** No resolver inside; proxy decides on host,
   port and resolved address.
5. **Depend on little.** There is no mature, permissive, cross-platform Rust sandbox crate we should adopt whole. Use
   `landlock`, `seccompiler`, system `bwrap`, `mxc-sdk`; write the profile compiler ourselves; borrow ideas (Apache-2.0,
   with NOTICE if code is copied) from codex and sandbox-runtime.
6. **Code mode: Monty first, rquickjs second, both in a separate sandboxed process.** Not V8 unless a product need appears.

## 2. macOS

| Question | Finding |
|----------|---------|
| `sandbox-exec` on 27.0 | Present, still "DEPRECATED" in a man page dated 2017, works [T]. `sandbox_init` marked deprecated since 10.8 in the SDK header [T]. Chrome, Codex, Claude Code, Gemini CLI, Cursor all rely on it [S, D]. No 27-specific change found; Apple could remove it at any release |
| Cost | `/usr/bin/true`: 1.55 ms plain vs 8.62 ms under a small profile (p50, n = 40) → ≈ 7 ms per spawn [T] |
| Write confinement | Write to a path outside via `..` and via a dangling symlink inside the workspace: both `EPERM` [T] (kernel resolves the vnode, unlike path-string checks) |
| Network | `(deny default)` without network rules: TCP and DNS both fail. Allowing only `localhost:PORT`: DNS still fails, other ports denied. Allowing the mDNSResponder socket: DNS **succeeds** → DNS exfil channel [T] |
| Keychain | With blanket `(allow mach-lookup)`, `security list-keychains` answers inside the sandbox (reading an item was not attempted) [T] → name Mach services one by one; deny `com.apple.SecurityServer`, `securityd`; but Codex allows SecurityServer/trustd for TLS when network is on [S], and srt flags `trustd.agent` as an exfil vector [S]. Prefer a tool CA bundle via env over trustd |
| Nesting | Parent with any differing profile → inner `sandbox_apply: Operation not permitted`, rc 71, in 4 of 4 shapes including parent `(allow default)` + one deny, and with `(allow system-mac-syscall)`. Identical allow-default profile in both: accepted [T] |
| LaunchServices | `open` inside the strict profile fails to find apps [T]. Antigravity's `(allow default)` profile was escaped through devfs mount + `open` [P]; `(deny default)` is mandatory |
| Alternatives | App Sandbox: would need every child binary signed with inherit entitlement; unsuitable (Cursor's conclusion too [D]). Apple `container` (Linux VMs, macOS 26+, Apple silicon): good optional VM backend, cannot run macOS toolchains. Endpoint Security: needs Apple-granted entitlement and root; no |

Profile rules to adopt: `(deny default)`; closed Mach-service and sysctl lists (start from Codex's base policy, itself
derived from Chrome's [S]); deny `file-write-unlink` on every ancestor directory of a protected path, otherwise the parent
can be renamed away (Codex does this for daemon sockets [S]); match protected names by regex so absent files are covered;
lower-case comparison and NFC/NFD on APFS (srt lower-cases everywhere [S]); pass paths as `-D` params, never by string
interpolation; hard-code the absolute path of `sandbox-exec`. Tag log denials to surface "blocked by sandbox" to the model;
Cursor reports that explicit denial reasons were needed to stop blind retries [D].

## 3. Linux

| Mechanism | State | Role for Axiom |
|-----------|-------|----------------|
| bubblewrap | LGPL-2.0+ **binary**, spawned not linked. 0.12.0 fixes CVE-2026-87766: setup-time symlink writes via `/oldroot` when creating files over attacker-controlled content [A], which is exactly a hostile workspace | Primary. Prefer distro `bwrap` found outside cwd (Codex rule [S]); probe behaviour, not only version (distros backport) |
| User namespaces | Ubuntu ≥ 23.10 restricts by AppArmor; 24.04 needs a bwrap profile loaded, 25.04+ ships one [search excerpts]; 26.04 unverified | Detect; explain the exact fix; **never** suggest disabling the sysctl globally; no fallback to unsandboxed |
| seccomp-BPF | Codex blocks `ptrace`, `process_vm_*`, `io_uring_*` (socket-filter bypass), `AF_VSOCK`, and in network-off mode `connect/bind/listen/...` [S]; srt blocks `AF_UNIX` creation + io_uring | `seccompiler` 0.5 (Apache-2.0 OR BSD-3), applied after the FS view is set; add `keyctl/request_key`, `TIOCSTI`, `userfaultfd`, `bpf`, nested `unshare`/`clone(NEWUSER)` |
| Landlock | Kernel docs describe ABIs to 12: 4 TCP, 5 ioctl, 6 signal + abstract-socket scoping, 9 pathname-socket control, 10 UDP, 12 namespace/capability rules [D]. Crate 0.4.8 covers ABI 10 [D] | Second layer inside bwrap; sole layer for engine-side helper processes. Cannot alone do "deny inside allow" (no negative rules); Codex now *rejects* Landlock-only for restricted filesystems because it could not isolate app-server sockets [S] |
| cgroup v2 / prlimit | Process, memory caps | Fork-bomb control (T39) |
| Network | `--unshare-net`, then UDS-bind-mounted proxy (srt, Codex). Codex also has an opt-in TUN-based transparent TCP with synthetic DNS `198.18.0.0/15` [S] | Start with env proxy + UDS bridge; transparent TCP later |
| Abstract/pathname sockets | Docker, D-Bus, systemd, Wayland, ssh-agent are escapes | Mount masks over `/run/user/$UID`, `/var/run/docker.sock` and bind aliases (Codex walks mount identity [S]) |
| Containers | Inside Docker bwrap often lacks userns; srt's `enableWeakerNestedSandbox` "considerably weakens security" [S] | Detect; treat the container as containment only on explicit user grant |

Details to copy: read-only `/` bind then writable roots, protected subpaths re-bound `ro`, missing protected components
masked by `/dev/null`, `--unshare-user --unshare-pid --new-session --die-with-parent`, fresh `/proc`, `--as-pid-1` [S].

## 4. Windows

| Option | Isolation | Cost / gaps |
|--------|-----------|-------------|
| **MXC** `processcontainer` (BaseContainer / PSEC) | OS-enforced FS allow + deny lists, network deny with loopback proxy, job object; no admin, no ACL edits [S, D] | Needs Windows 11 build ≥ 26100.9278 / 26200.9278 / 26300.9550 / 28000.2804 [D]. Known gaps from srt: MSYS2/Git Bash crash, no write-only deny, Schannel does not trust a MITM CA, `%LOCALAPPDATA%` is per-command, UI must be on, stalls under parallelism [S]. Codex and srt both probe per request and refuse the AppContainer tier [S]. Telemetry: §1.3 |
| Dedicated local account + WFP + restricted token (Codex "elevated", srt-win) | Good FS boundary by SID; WFP keyed on SID [S] | One UAC prompt, machine-wide state, ACL stamping, per-user toolchains unreadable. Documented holes: anonymous or derived tokens on a thread escape the WFP fence; SMB via redirector; DNS via Dnscache [S] |
| Restricted token / Low IL only (Codex `RestrictedToken`, Gemini `GeminiSandbox.cs` + `icacls` Low) | Write confinement partial; no network fence | Leaves persistent integrity labels on user files [D]. Reject as a boundary |
| AppContainer / LPAC | Strong, designed for browsers | Breaks dev tools; both vendors rejected it for general shells [S]. Fine for **our own** helpers (code-mode host, MCP fetchers): `rappct` 0.13 MIT, single maintainer |
| WSL2 | Full Linux backend | Files on `\\wsl$`/`/mnt/c` are slow; `/mnt` interop (`cmd.exe`) is an escape: disable `interop.enabled` + `appendWindowsPath` for the sandbox distro, plus bwrap inside |
| Windows Sandbox / Hyper-V | VM | Pro/Enterprise only, seconds to start; MXC marks it experimental [D] |

Position **[I]**: M2 ships Windows with (a) MXC when the probe passes, (b) WSL2 backend, (c) otherwise read-only + per-command
approval with a permanent "not contained" badge. Do not write a WFP service before there is evidence (a)+(b) do not cover users.
Non-negotiable everywhere: ConPTY child in a kill-on-close job, explicit inherited-handle list, absolute image path, reject
reserved names, ADS (`:`), 8.3 short names, trailing dots/spaces, `\\?\` and UNC in tool paths (Cursor CVE-2025-64107/64108 [A]).

## 5. How others do it

| | Codex | Claude Code / srt | Gemini CLI | Cursor |
|---|---|---|---|---|
| Default | Sandboxed shell | **Off by default**; when enabled and unavailable runs unsandboxed unless `failIfUnavailable` [D] | Opt-in `-s`; default profile permissive-open (network allowed) [D] | On in auto-run [D] |
| Coverage | Shell + FS helper + exec-server | Shell only; file tools, hooks, MCP, LSP outside [D] | Tool sandbox managers per OS, or whole CLI in Docker/Podman/runsc/LXC | Terminal |
| macOS | Seatbelt, deny default [S] | Seatbelt, generated | Seatbelt, deny default + `(import "system.sb")` [S] | Seatbelt, regex denies for `.git/hooks`, `.vscode`, `.cursor` [D] |
| Linux | bwrap + seccomp | bwrap + socat + seccomp | bwrap | Landlock + seccomp + overlay [D] |
| Windows | Restricted token / elevated users + WFP / MXC | none in Claude Code; srt-win alpha, MXC backend [S] | Low IL + C# helper | WSL2 [D] |
| Network | Rust proxy: HTTP 3128 + SOCKS5 8081, allow/deny, GET-only "limited" mode, MITM hooks, credential substitution; `*` rejected; resolved private IPs blocked even if allow-listed [S] | Node proxy; per-command allow lists by 128-bit id in proxy username; credential sentinels; resolved-address guard; allow-list starts empty [S] | Proxied profiles optional | Proxy, prompts |
| Protected | `.git`, gitdir target, `.agents`, `.codex`; glob unreadables [S] | `.gitconfig`, `.gitmodules`, shell rc, `.ripgreprc`, `.mcp.json`, `.vscode`, `.idea`, `.claude/{commands,agents}`, git hooks/config [S] | governance files `.gitignore`, `.git`, … [S] | list above + `.code-workspace` |
| Policy | Starlark `prefix_rule` with inline `match/not_match` tests; strictest wins [S] | JSON allow/ask/deny patterns | TOML, 5 priority tiers Admin > User > Workspace > Extension > Default [S] | Allow-list, documented best-effort [search excerpt] |

Lessons: an empty allow-list must mean "deny all" (CVE-2025-66479 [A]); cwd must not become a root (CVE-2025-59532 [A]);
non-existent protected files must still be protected (CVE-2026-25725 [A]); seven of seven Pillar findings were *handoff* bugs [P].

## 6. Network proxy design **[I]**

- In-process in the engine, HTTP CONNECT + SOCKS5h, bound to a UDS (Linux), loopback port allowed by profile (macOS/Windows).
  Per-command random credential in the proxy username identifies the command → approvals and audit attribute correctly.
- Decision input: `{command id, agent chain, host (IDNA-normalised), port, resolved IPs, method?, path?, taint}`.
  Resolve in the proxy, pin the resolved address for dial, reject loopback/link-local/RFC1918/ULA/CGNAT/metadata by default,
  re-decide on every redirect. Deny beats allow; `*` is rejected.
- Default lists empty. "Package registry" preset = named hosts, GET/HEAD only.
- TLS interception is **off** by default; opt-in per host for method/path rules and credential injection, CA key in memory only.
- Credential sentinels: child sees `axm_sentinel_…`; the proxy substitutes only for the bound origin. Useful for `gh`, registries
  and for harness API keys when the harness supports a base URL.
- Crates: `hyper` + `tokio-rustls` + `rcgen` by hand (~2–3 kLOC **[I]**). `hudsucker` (MIT/Apache) for MITM if needed; `rama` is
  capable but 0.x and very large.

## 7. Permission policy language

Shell parsing pitfalls, each with a shipped CVE (ids in threat-model T16): command substitution, `$(( $(cmd) ))`, `${x@P}`,
assignment prefixes (`PAGER=… git log`), process substitution, heredocs, zsh-only syntax (`>!`, `=cmd`), `find -exec`,
`sed -n 'w f'`/`e`, `rg --pre`, `git -c`, `xargs`, `env`, `nohup`, `bash -c`, aliases and rc files, CRLF, Unicode spaces,
Windows `^` and `%VAR%`, PowerShell `$()` inside double quotes.

Codex's approach is the best seen [S]: parse with `tree-sitter-bash`; accept only a closed list of node kinds (program, list,
pipeline, command, word, string, raw_string, number, concatenation) and operators (`&& || ; |`); any redirection, expansion,
subshell or parse error → not analysable → ask. At `a0d5b0a` the crate holds a *dangerous*-command
classifier and no built-in safe-command list; PowerShell gets its own parser. The result classifies; the sandbox contains.

Proposal **[I]**: typed, declarative, *total* (no loops, no I/O), evaluated in the engine, TOML on disk:

```toml
version = 1
[[rule]]
id = "cargo-test"
tool = "shell"
argv = ["cargo", ["test", "check", "clippy"], { rest = "any" }]   # ordered; list = alternatives; `rest` must be explicit
deny_args = ["--config", "-Z*"]                                   # argv options that change what runs
run_in = "workspace-write"                                        # names a sandbox profile, never "none"
net = { preset = "registry-read" }
decision = "allow"                                                # allow | ask | deny
match = ["cargo test -p foo"]                                     # load-time tests; failure rejects the file
not_match = ["cargo test --config build.rustc-wrapper='x'"]
```

Rules: (1) decisions compose by `deny > ask > allow`, no priorities; (2) scopes only tighten downward (admin ∩ user ∩ workspace
∩ session ∩ agent), workspace files may contain only `ask`/`deny`; (3) `allow` always names a sandbox profile; (4) program
matched by resolved absolute path from a trusted set, not basename; (5) compound commands are allowed only if every part is;
(6) "always allow" writes an argv-prefix rule with the tests auto-generated and shows it before saving; (7) same language for
MCP tools (`server`, `tool`, schema hash, JSON-pointer predicates), file paths and hosts; (8) decisions are explainable: every
answer returns the rule ids. Cedar (`cedar-policy` 4.15, Apache-2.0) was considered: analysable and total, but argv sequence
matching is awkward and it adds weight; revisit if organisation policy arrives. Starlark in config is more power than needed and
is executable configuration. Rego (`regorus`, MIT AND Apache-2.0 AND BSD-3-Clause) is unfamiliar to users.

## 8. Reusable crates

| Crate | Version (date) | Licence | Verdict |
|-------|----------------|---------|---------|
| `landlock` | 0.4.8 (2026-11-20) | MIT OR Apache-2.0 | Use |
| `seccompiler` | 0.5.0 (2025-03) | Apache-2.0 OR BSD-3 | Use |
| `mxc-sdk` | 1.1.0 (2026-11-14) | MIT | Evaluate on Windows only; 7.5 MB crate, telemetry, very young |
| `cap-std` | 4.0.3 (2026-08-20) | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Use for handle-relative I/O; verify Windows reparse behaviour (wasmtime GHSA-f2pq, Nov 2026, was a Windows drive-prefix escape) |
| `tree-sitter` + `tree-sitter-bash` | 0.27.2 / 0.25.1 | MIT | Use |
| `process-wrap` | 11.0.0 (2026-11-24) | Apache-2.0 OR MIT | Use for process groups / job objects; `command-group` is its stale predecessor |
| `nono` | 0.82.0 (2026-11-14) | Apache-2.0 | Closest whole product (Landlock/Seatbelt + proxy); study, do not depend (pre-1.0, weekly releases) |
| `rappct` | 0.13.3 (2025-10) | MIT | Only if we AppContainer own helpers |
| `libseccomp` | 0.4.0 | crate MIT/Apache, **links LGPL C library** | Avoid; seccompiler suffices |
| `pathrs` | 0.2.6 | MPL-2.0 OR LGPL-3.0+ | Not needed; `rustix::openat2` instead |
| `hakoniwa` | 1.8.1 | LGPL-3.0 | **Reject** (static linking obligations) |
| `birdcage` | 0.8.1 | GPL-3.0-or-later, archived | **Reject** |
| `yash-syntax` | 0.25.0 | GPL-3.0-or-later | **Reject** |
| `extrasafe`, `openat2`, `sandbox-runtime` (3rd-party, 22 downloads/90 d) | – | MIT | Stale or not credible |

## 9. Code-mode runtimes

Job: run a short model-written script that calls tools as functions, inside the caller's capability set. Every host call goes
through the same policy engine as a direct tool call. Isolation column assumes **no** OS sandbox around it.

| Runtime | Startup | Isolation | Size | Licence | Snapshot/suspend at host call | Model fluency | Risk |
|---------|---------|-----------|------|---------|------------------------------|---------------|------|
| **Monty** 1.6.0 (Rust Python subset) | 0.8 ms pooled, 5 ms new pool [vendor, M3 Max] | Language-level, no ambient authority, no FFI; worker subprocess; time/memory limits **off unless set** | crate 2.2 MB; worker ≈ 2 MB RSS [vendor] | MIT | **Yes**, KB-sized, forkable | High (Python) but subset: no 3rd-party libs, limited stdlib; type checker gives errors before run | 1.0 in 2026; fast churn; 0 published advisories so far; bounty programme; MSRV 1.98 |
| **QuickJS-ng** via `rquickjs` 0.14.0 | < 300 µs runtime life cycle [upstream claim] | C engine; 10 published advisories, several heap corruptions, latest batch 2026-07-31 [A]; memory limit + interrupt hook | ≈ 1 MB **[I]** (upstream: 367 KiB x86 hello world) | MIT | No | High (JS; TS needs stripping) | Memory-unsafe C → must be out of process |
| **V8** `v8` 152.2 / `deno_core` 0.413 | ms-range with snapshot **[I]** | Best-studied JIT; V8 sandbox flag; still frequent CVEs. Codex runs it in a separate `code-mode-host`, offers `--jitless`, and carries flags disabling Maglev/Turbolev for a V8 bug [S] | 36 MB crate; tens of MB binary **[I]** | MIT + V8 BSD-3 | No | Highest (TS/JS) | Fortnightly security treadmill; Codex maintains a fork |
| **Wasmtime** 51 + guest interpreter | 16 ms with precompiled CPython [Monty's bench] | Strong in-process boundary by design; 71 advisories in 2026 (5 critical, 4 high, 42 medium, 20 low), incl. miscompilation escapes and WASI path escapes [A] | 10–20 MB + guest **[I]** | Apache-2.0 WITH LLVM-exception | No (component async) | Depends on guest | Heavy, fast-moving MSRV (1.97) |
| **Starlark** 0.14.3 (Meta, Rust) | 1.3 ms [Monty's bench] | Deterministic, hermetic; no `while`, recursion limited | crate 0.7 MB | Apache-2.0 | No | Medium: looks like Python, models hit missing features (`try`, `while`, classes) | Low |
| **RustPython** 0.6 | unmeasured | Full VM with stdlib FFI surface; not designed as sandbox | large | MIT | No | High | Reject |
| **Lua** `mlua` 0.12 (Luau) | sub-ms **[I]** | Luau has a real sandbox story; others weak | < 1 MB **[I]** | MIT | Coroutines (not serialisable) | Medium-low | Fine, low fluency |
| **Rhai** 1.27 / **Boa** 0.22 | sub-ms / ms **[I]** | Memory-safe Rust | 0.5–1 MB | MIT/Apache; Unlicense OR MIT | No | Low / High | Boa: slower, safe alternative to QuickJS |

Fluency evidence is weak: vendors split (Anthropic programmatic tool calling = Python, Cloudflare code mode = TypeScript, Bifrost =
Python-as-Starlark) and no independent head-to-head was found. Token savings 50–90 % are vendor-reported and shrink to
nothing with few tools. **Decide by our own eval.**

Recommendation **[I]**: put `ScriptHost` behind a trait in a child process with no FS, no network and only the pipe to the
engine (Seatbelt deny-all / empty-netns bwrap / LPAC). Prototype Monty (suspend/resume at a host call maps directly onto
durable approvals) and rquickjs with the same 20-task eval. Run scripts with call-count, wall-clock, memory and output caps;
never expose `eval` of host code; a script's capability set is the calling agent's, intersected with the script's declared tools.

## 10. Unverified / owed

Linux and Windows behaviour (no machine used); MXC deny semantics and parallel stalls; Ubuntu 26.04 userns default; V8 and
QuickJS start-up on our hardware; whether Claude Code/Codex accept "sandbox disabled, externally contained" cleanly in every
mode (Codex source has an explicit `ExternalSandbox` policy [S]; Claude Code has `sandbox.enabled=false` [D]) and whether doing so loosens
their prompts; private `responsibility_spawnattrs_setdisclaim`; the reported September 2026 Codex escape.

## 11. Sources

Codex https://github.com/openai/codex (`codex-rs/{linux-sandbox,sandboxing,windows-sandbox-rs,mxc-sandbox,network-proxy,execpolicy,shell-command,code-mode-runtime,process-hardening}`) ·
sandbox-runtime https://github.com/anthropics/sandbox-runtime · https://code.claude.com/docs/en/sandboxing ·
Gemini CLI https://github.com/google-gemini/gemini-cli (`docs/cli/sandbox.md`, `packages/core/src/{sandbox,policy}`) ·
Cursor https://cursor.com/blog/agent-sandboxing · MXC https://github.com/microsoft/mxc (`README.md`, `docs/telemetry.md`, `docs/backends/process-container/os-version-support.md`) ·
Landlock https://docs.kernel.org/userspace-api/landlock.html · bubblewrap GHSA-pxhw-h44j-8pfx ·
Ubuntu https://discourse.ubuntu.com/t/understanding-apparmor-user-namespace-restriction/58007 ·
Monty https://github.com/pydantic/monty (`docs/alternatives.md`, `docs/security.md`) · Apple container https://github.com/apple/container ·
[P] https://www.pillar.security/blog/the-week-of-sandbox-escapes · crates.io API, queried 2026-11-24.
