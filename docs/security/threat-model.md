# Axiom threat model

Status: M0 draft, 2026-11-24. Nothing here is implemented. Companion: `docs/research/sandboxing.md` (mechanisms),
`docs/research/harness-integration.md` (delegated harnesses). Evidence tags: **[A]** vendor advisory read, **[S]** source
read, **[T]** tested on macOS 27.0 (26A428), **[R]** secondary reporting, **[I]** inference or design proposal.

Milestone names are **proposals [I]**, to be reconciled with the roadmap:
**M1** engine + UI + one provider, read-only agent · **M2** write/shell tools, sandbox, policy · **M3** MCP and delegated
harnesses · **M4** multi-agent orchestration and code mode · **M5** extensions, updater, signed releases.

## 1. Conclusions

1. **The boundary is "what a trusted host component later reads", not the agent process.** Pillar's seven July 2026
   escapes (Cursor, Codex, Gemini CLI, Antigravity) broke no kernel sandbox: the agent wrote something the host later ran,
   loaded or trusted [P1]. Axiom's own git status, file watcher, LSP, syntax worker and UI are those host components.
2. **Prompt injection is assumed to succeed.** No control below relies on the model refusing. Filters and classifiers are
   telemetry, never an enforcement point.
3. **Permission prompts are consent, not containment.** String-level allow-lists for shell have failed in every product
   examined (§7). Containment is the OS sandbox plus a network proxy; the policy engine decides *which* sandbox.
4. **Delegated harnesses are outside our gate.** ACP says the agent runs tool calls itself and *MAY* ask [ACP]. Whatever a
   harness does happens unless the OS stops it, and on macOS **our Seatbelt profile around a harness disables the harness's
   own Seatbelt sandbox** [T, sandboxing.md §2]. Exactly one layer must own containment per session, recorded and shown.
5. **Multi-agent is a capability-passing problem.** Messages carry data, never authority. Only the engine mints authority.
6. **File tools need handle-relative I/O.** Claude Code (3 advisories), Cursor and Zed (1 each) show that check-then-open
   on path strings is a race that sandboxed code in the same workspace can win [CC1, CC2, CC10, CU2, Z1].

## 2. Assets

| ID | Asset | Why it matters |
|----|-------|----------------|
| A1 | User files outside the workspace; OS account | SSH keys, browser profile, shell rc → persistence |
| A2 | Workspace source, history, remotes | Integrity (backdoor), confidentiality (exfiltration) |
| A3 | Provider credentials (API keys, OAuth refresh tokens), MCP tokens, git credentials | Direct financial and account loss; s1ngularity and QUIETVAULT targeted exactly these [R1, R2] |
| A4 | Permission grants, trust decisions, policy files | Changing them turns every other control off |
| A5 | Transcripts, memories, context packs, embeddings | Contain A2 and A3 fragments; memory is also an injection persistence point |
| A6 | Audit log | Needed to answer "what did the agent do" |
| A7 | Release signing keys, update channel | One compromise reaches every user |
| A8 | Spend, quota, compute | Denial of wallet |

## 3. Actors

| Actor | Capability | Trust |
|-------|-----------|-------|
| User | Grants authority through native UI only | Trusted (may be fatigued or deceived) |
| Model | Chooses tool calls; text is untrusted | Untrusted |
| Content author | Repo files, issues, web pages, tool output, package metadata | Hostile |
| MCP server / extension / skill author | Runs code or supplies descriptions | Untrusted until granted; may turn hostile later (rug pull) |
| Delegated harness | Own tools, own config, own credentials | Semi-trusted binary driving untrusted model output |
| Sandboxed child code | Arbitrary code inside the box, concurrent with the engine | Hostile |
| Local web page in user's browser | HTTP/WebSocket to loopback, custom URL schemes | Hostile |
| Other local OS user | Shared machine, shared `/tmp`, machine-wide config dirs | Hostile |
| Same-user malware | Full user rights | **Out of scope** except: do not hand it extra rights (TCC, keychain ACLs) and no ready-made "YOLO" entry point |
| Provider / network | Sees prompts; MITM | Provider trusted for confidentiality by user choice; network hostile |

## 4. Trust boundaries and data flow

```
 user ─► [Flutter UI] ═B1═ [engine] ═B2═► provider APIs (TLS)
              │                │ ═B3═► delegated harness (claude, codex, ACP …) ─► its own provider + tools
              │                │ ═B4═► MCP servers (stdio child | Streamable HTTP)
              │                │ ═B5═► extensions / skills / code-mode scripts
              │                │ ═B6═► tool runner ─► [OS sandbox] ─► shell, build, tests
              │                │              └─► [egress proxy] ═B7═► internet
              │                ├ ═B8═ storage (SQLite, blobs) · OS credential store
              └ renders untrusted text (B9)     └ ═B10═ updater / registries
                                        B11: agent ◄─engine-mediated messages─► agent
```

| B | Crossing | Data in → | Enforced by |
|---|----------|-----------|-------------|
| B1 | UI ↔ engine | Commands, approvals / events | Inherited stdio pipe or private UDS / named pipe; per-launch token; UI holds no secrets |
| B2 | engine → provider | Prompts / tool calls | TLS, pinned endpoint per credential; endpoint never from workspace config |
| B3 | engine → harness child | Prompt, cwd, env / events, permission asks | OS sandbox around the child, scrubbed env, pre-spawn trust gate |
| B4 | engine ↔ MCP | Args / results, descriptions, elicitation | Sandboxed stdio child; hash-pinned tool schema; size caps |
| B5 | engine ↔ extension | Host calls | Manifest capabilities; out of process; no ambient authority |
| B6 | engine → shell tool | argv, cwd, env | Policy → sandbox profile → spawn; engine-owned cwd root |
| B7 | sandbox → net | Bytes | No direct route; proxy with host + resolved-IP checks |
| B8 | engine ↔ disk | Rows, blobs | 0700 dirs, 0600 files, outside all writable roots; secrets only in OS store |
| B9 | any text → pixels | Markdown, ANSI, SVG, Mermaid | No network fetch, no raw escape passthrough, sanitised links |
| B10 | internet → engine | Binaries, catalogues | Signature + version monotonicity |
| B11 | agent → agent | Message, artefact reference | Provenance labels; capability intersection |

## 5. Threats

Columns: threat · STRIDE · mitigation · residual · **by** (milestone) · test. Evidence letters refer to §9.

### 5.1 Malicious repository content

| ID | Threat (evidence) | S | Mitigation | Residual | By | Test |
|----|-------------------|---|-----------|----------|----|------|
| T1 | Repo config executes or re-configures before trust: hooks, MCP definitions, `defaultMode: bypassPermissions`, `ANTHROPIC_BASE_URL` (CVE-2025-59536, -65099, CVE-2026-21852, -33068 [CC3–CC6]; Check Point [R3]) | E | Workspace config is **data-only** until trust; can only *narrow* policy (SI-5); cannot set endpoints, env, commands, permission mode or binaries. Trust shows the exact executable content and is bound to its hash | User trusts a hostile repo | M1 | Corpus of hostile repos opened with a syscall tracer: zero exec, zero network before trust |
| T2 | Trust keyed on spoofable identity: worktree `commondir` (CVE-2026-40068 [CC7]), path-prefix collision (CVE-2025-54794 [CC8]) | S | Trust key = canonical path + device/inode + git common dir verified by back-pointer; component-wise prefix | Directory replaced after trust: inode changes → re-prompt | M1 | Property test on prefix/`..`/case/NFC variants; spoofed worktree fixture |
| T3 | Host git runs repo-supplied code: `core.fsmonitor`, hooks, `diff.external`, filters, aliases, `sshCommand`; git dir not named `.git` (goose CVE-2026-72718, Zed CVE-2026-44465, Cursor CVE-2026-26268, GitPwned [G1, Z2, CU3, P1]) | E | Every engine git call goes through one wrapper: absolute binary, `-c core.fsmonitor=false -c core.hooksPath=<empty dir> -c protocol.ext.allow=never --no-ext-diff --no-textconv`, `safe.bareRepository=explicit`, scrubbed env, **and runs inside a no-network sandbox** (read-only for queries), because flags cannot disable every key (filters, includes) | New git config keys | M1 | Fixture per dangerous key; CI diff of `git config` key list vs reviewed list on git upgrade |
| T4 | Binary planting: `git.exe` in cwd (CVE-2026-88066 [CC9]), interpreter in repo venv (CVE-2026-73217 [CU4]), language servers from workspace settings (Zed CVE-2025-68432 [Z3]) | E | Executables resolved once from a user-level path with workspace and writable roots excluded; Windows spawn with absolute image path and `SetDefaultDllDirectories`; no interpreter auto-discovery outside sandbox | User PATH includes a writable dir | M1 | Plant fixtures on 3 OSes |
| T5 | Parser attacks on engine: archives (Zed zip-slip, tar symlink CVE-2026-27800/27976 [Z4]), huge files, regex/grammar bombs | D/T | Reject absolute/`..`/link entries; size, depth and time caps; grammar parsing in a killable worker | Bugs in C grammars | M2 | Fuzz targets; zip-slip corpus |
| T6 | Agent instruction files (`AGENTS.md`, `.claude/`, `.cursor/`) as injection | T | Loaded only after trust, shown in context inspector with origin and hash; changed-since-trust → re-prompt; agent writes to them are protected paths (T14) | Trusted repo later compromised by a pull | M1 | Pull that edits AGENTS.md → banner + not loaded silently |

### 5.2 Prompt injection

| ID | Threat | S | Mitigation | Residual | By | Test |
|----|--------|---|-----------|----------|----|------|
| T7 | Via tool output, files, web pages (EchoLeak CVE-2025-32711, CamoLeak [R4, R5]) | T/I | Treat as successful; sandbox + egress control; **taint**: once a session has read untrusted content *and* holds private data, exfil-capable actions need approval even if pre-allowed ("lethal trifecta" rule) | In-workspace harm: bad code committed | M2 | Injection suite replayed per release, scored on *effect*, not refusal |
| T8 | Via MCP tool descriptions / schemas / rug pull (CVE-2025-54136 MCPoison; Zed CVE-2026-25805 args not shown [CU5, Z5]) | T | Description + schema hash pinned at approval; change → disabled until re-approved with diff; approval shows full args, control characters escaped | Semantics hidden in long text | M3 | Mutating mock server; golden of approval dialog |
| T9 | Via other agents' messages / shared artefacts (ADK PR agents, Rehberger cross-agent [R6, R7]) | E | Messages delivered as labelled data with origin chain and taint union; no message can be parsed as an approval, config or tool call; receiver keeps its own capabilities | Persuasion inside receiver's own grant | M4 | Two-agent escalation suite: low-cap agent tries to make high-cap agent act; assert approval required |
| T10 | Memory / compaction / resumed transcript poisoning (persists) | T | Memory entries carry provenance; tainted content cannot become instruction-tier memory without user accept; compaction preserves taint | Subtle factual poisoning | M4 | Poison → compact → resume → assert label still present |
| T11 | Hidden text: bidi, zero-width, Unicode tag block, ANSI (Trail of Bits 2025 [R8]) | S | Visible escapes for C0/C1, bidi controls, tag block, zero-width in approvals, diffs, tool args; homoglyph warning on hosts | Visual confusables in prose | M1 | Golden images for each class |
| T12 | Approval spoofing: model text imitating UI, argument truncation, fatigue | S | Approvals only in native chrome that transcript content cannot draw into; never truncate the security-relevant part; batch by capability; no default-focused "Allow" | Habituation | M2 | Widget tests; 10k-char command still shows tail |

### 5.3 Filesystem and shell

| ID | Threat | S | Mitigation | Residual | By | Test |
|----|--------|---|-----------|----------|----|------|
| T13 | Symlink / TOCTOU on engine file tools (CVE-2026-88968, -103435, -39861; Cursor CVE-2026-50549; Zed CVE-2026-27967 [CC1, CC2, CC10, CU2, Z1]) | T/E | `openat2(RESOLVE_BENEATH\|NO_MAGICLINKS)` on Linux, `openat` component walk with `O_NOFOLLOW` (macOS), reparse-point-refusing handle walk (Windows); write temp + rename **by directory handle**; canonicalisation failure ⇒ deny. Alternative: run the writer *inside* the same sandbox (preferred where available) | Hard links to outside inodes: refuse `st_nlink>1` on protected files | M2 | Racing swapper thread, 10⁶ iterations per OS; dangling-link, unreadable-parent, junction, 8.3 and ADS cases (Cursor CVE-2025-64107/64108) |
| T14 | Writes to files a host component executes: `.git/hooks`, `.git/config`, shell rc, `.envrc`, `.vscode/tasks.json`, `.idea`, `.code-workspace`, `.mcp.json`, `.claude`, `.cursor`, `.codex`, `.gemini`, `.axiom`, venv/bin, `node_modules/.bin`, `package.json` scripts, `Makefile`, `build.rs`, `conftest.py` (CVE-2026-25725, -48124; Antigravity tasks.json [CC11, CU1, P1]) | E | Two tiers. **Protected** (deny-write, present *or absent*): VCS metadata, agent config, shell/IDE task config. **Review-on-write**: build scripts and manifests, flagged in diff before anything outside the sandbox runs them. Name-independent detection of git dirs | Anything the user later runs unsandboxed (`npm test` in their own terminal) | M2 | Matrix of paths × absent/present/symlink/rename-parent × OS |
| T15 | Agent-controlled cwd / writable root (Codex CVE-2025-59532, Cursor CVE-2026-50548 [CX1, CU6]) | E | Roots come from the session record only; model `cwd` must resolve beneath an existing root and grants nothing | — | M2 | cwd fuzz |
| T16 | Shell-string policy bypass: substitutions, env assignment, arithmetic, here-strings, `sed e`, `find -exec`, `rg --pre`, zsh `>!` (CVE-2025-66032, -64755, -58764, -54795, CVE-2026-24053, -24887, -25722, -25723; Zed CVE-2026-44462/44463/44466; Cursor CVE-2026-22708 [CC12, Z6, CU7]) | E | Auto-allow only if tree-sitter parse has no error **and** every node kind is on a closed allow-list; else prompt. Executed by a known shell with no rc files and scrubbed env. Sandbox is the actual limit | Parser differential bash vs zsh/PowerShell | M2 | Advisory regression corpus; differential fuzz vs real shell `set -n`/AST |
| T17 | Destructive but "in scope": `rm -rf`, force-push, `DROP`, cloud CLIs | T/D | Per-turn checkpoint outside the workspace root; force-push/remote-mutating verbs need fresh approval; cloud credentials unreadable in sandbox | External side effects are not undoable | M2 | Kill mid-command; restore byte-identical |
| T18 | Privileged local daemons: docker/podman socket, systemd user bus, launchd, Task Scheduler, WSLg, `ssh-agent`, LaunchServices `open`, AppleEvents (GHSA-v4xv [CU8], Antigravity [P1]) | E | Default-deny Unix sockets and Mach lookups by name; mask socket dirs even via bind mounts; Windows: separate principal or BaseContainer | Newly added OS service | M2 | Escape probe list run in CI inside each sandbox |
| T19 | Sandbox unavailable → silent unsandboxed run (Claude Code's documented default [D1]) | E | **Fail closed**; unsandboxed mode is an explicit, per-session, visibly badged grant | User chooses it | M2 | Remove bwrap / break userns / rename binary ⇒ refuse |

### 5.4 Network and exfiltration

| ID | Threat | S | Mitigation | Residual | By | Test |
|----|--------|---|-----------|----------|----|------|
| T20 | DNS exfil (CVE-2025-55284 [CC13]); on macOS a profile that allows only the mDNSResponder socket still resolves names; blanket `deny network*` does not **[T]** | I | No resolver in the sandbox: macOS deny the mDNSResponder socket, Linux empty netns, Windows documented gap (Dnscache) | Windows native until MXC-class isolation is verified | M2 | `getaddrinfo` canary per OS in CI |
| T21 | Allowed-domain abuse: huggingface path counter (CVE-2026-54316 [CC14]), GitHub gists, package registries, domain fronting, wildcard DNS to LAN | I | Policy on host **+ port + resolved IP class**, optional method/path rules under TLS interception; registries via read-only mode (GET/HEAD) with dependency cache; no pre-approved hosts by default | URL-path encoding to an allowed host | M2 | SSRF/rebinding suite, redirect re-evaluation, IPv4-mapped/octal/IDN forms |
| T22 | Package install scripts (postinstall) exfiltrate or escape | E | Install runs sandboxed with registry-only egress and no credentials; `--ignore-scripts` default, elevate per package | Malicious code at test time, inside sandbox | M2 | Malicious package fixture |
| T23 | UI renders remote images/SVG/Mermaid/link previews (Cursor CVE-2025-54132, -61589 [CU9]; EchoLeak) | I | Renderer has **no** network capability; remote images become inert chips showing host + full URL; link open needs click and shows the decoded URL; `file:`, `smb:`, custom schemes blocked (UNC → NTLM leak) | User clicks | M1 | Golden + loopback canary server sees zero hits on a hostile-markdown corpus |
| T24 | Engine-side fetch tools as SSRF / exfil (CVE-2026-24052 [CC15]) | I | Same proxy and policy object as the sandbox; loopback, RFC1918, link-local and metadata IPs denied after resolution; redirects re-checked; URL userinfo/fragment stripped | — | M2 | As T21 |
| T25 | Provider endpoint substitution leaks key (CVE-2026-21852 [CC5]; rmcp header leak on redirect CVE-2026-64684 [M1]) | I | Credential bound to origin; HTTP clients follow no cross-origin redirect with auth; base URL only from user scope | User mis-sets URL | M1 | Redirect-to-canary unit tests |

### 5.5 MCP, extensions, dependencies, updates

| ID | Threat | S | Mitigation | Residual | By | Test |
|----|--------|---|-----------|----------|----|------|
| T26 | stdio MCP server = arbitrary code (postmark-mcp BCC backdoor [R9]; `npx -y` unpinned; server.json args/env GHSA-pp85/GHSA-rwv7 [M2]) | E/I | Install resolves to exact version + integrity hash; launch is sandboxed with per-server FS/net capabilities and only its own secret; full argv+env shown | Data the user grants it | M3 | Hostile server: reads `$HOME`, other servers' secrets, raw network → denied |
| T27 | Remote MCP: OAuth mix-up, authorization URL → shell (GHSA-q978 PowerShell, TS-SDK CVE-2026-104850), SSRF in metadata (GHSA-c9xm/5p52), unbounded bodies (GHSA-c9f9) [M1–M3] | S/E/D | RFC 9207 issuer check, RFC 8707 resource, PKCE, loopback redirect with state; browser opened through OS API with scheme allow-list, never a shell; SSRF guard on resolved address; body/event caps; rmcp ≥ 3.9 | SDK churn: 8 rmcp advisories in 7 months | M3 | Malicious authorization-server fixture |
| T28 | Tool shadowing / name collision between servers | S | Tools namespaced `server/tool`; built-ins unshadowable; grants keyed on server identity + schema hash | Model picks wrong server | M3 | Collision tests |
| T29 | Elicitation / sampling abuse (phishing for secrets, spend) | S | Elicitation in native chrome naming the server; secrets fields refused; sampling off by default, budgeted, no tools, no context | — | M3 | Mock server |
| T30 | Malicious skill / extension (ClawHub: 341 of 2,857 audited [R10]); pin bypass (CVE-2026-86063 [CC16]) | E | Content-addressed install (hash of the tree, not a ref); out of process, declared capabilities; scripts inside skills run only in the sandbox; permission-increasing updates held | Skill text is still prompt content | M5 | SHA-named branch fixture; capability diff |
| T31 | Compromised Rust/Dart dependency or build (Cline CLI 2.3.0, s1ngularity, worms of 2026 [R1, R11, R12]) | T | Lockfiles, `cargo-deny`/`cargo-vet`, Dart licence + hash check, 7-day cooldown, no build-time network, few proc-macros, OIDC publishing, protected CI with no agent-triage workflow holding secrets, SBOM + provenance | Upstream compromise past cooldown | M1→M5 | CI gates; reproducibility check on engine binary |
| T32 | Updater: unsigned, downgrade, cross-site trigger (OpenCode GHSA-632h [O1]) | T/E | Sparkle/WinSparkle-class signed feed with offline Ed25519 key, monotonic versions, fixed package identity; update never reachable from IPC with caller-supplied target | Key theft | M5 | Tampered, downgraded, expired feeds |
| T33 | Machine-wide config planted by another user (CVE-2026-35603 [CC17]) | E | Read `%ProgramData%`/`/etc` config only if owner is admin/root and not user-writable | — | M2 | ACL fixture |

### 5.6 Agents, harnesses, workspaces

| ID | Threat | S | Mitigation | Residual | By | Test |
|----|--------|---|-----------|----------|----|------|
| T34 | Confused deputy: child asks parent/sibling to act, or widens itself by spawning a harness with looser flags | E | `child ⊆ parent` for every spawn incl. delegated harness; flags like `--yolo`, `--dangerously-skip-permissions` mapped to capabilities and checked; approvals name the *originating* agent chain; grants are non-transferable | User grants broad scope | M4 | Lattice property tests; spawn graph fuzz |
| T35 | One agent rewrites another's config to free it (Rehberger [R7]) | E | All known agent config dirs (workspace **and** home) protected for every agent; pre-spawn hash check of harness config | Unknown harness config location | M3 | Per-harness path list tests; unknown → read-only HOME |
| T36 | Harness executes tools without asking (ACP *MAY* [ACP]) | E | Session shows containment owner: *Axiom sandbox* / *harness sandbox* / *none*; none requires explicit grant; callbacks treated as reports | Harness credentials readable by its own tools | M3 | Fake ACP agent that never asks; attempts escape |
| T37 | Cross-workspace leakage: shared caches, memory, search index, prompt cache, temp dirs, harness global history | I | Workspace id on every row and index; separate temp, HOME overlay and caches per workspace; memory default workspace-scoped; no cross-workspace retrieval without grant | Provider-side caches | M2 | Two-workspace canary strings; assert never retrieved, rendered or sent |
| T38 | Concurrent agents in one tree sabotage each other / race | T | Worktree per agent; worktree name validation (CVE-2026-55607 [CC18]); engine-only merge | Merge review skipped | M4 | Names `.git`, `..`, reserved, long, unicode |
| T39 | Runaway spawn / denial of wallet | D | Depth, fan-out, token, time and spend budgets inherited by intersection; kill tree (process group / job object / cgroup) | — | M4 | Fork bomb; recursive agent |

### 5.7 IPC, UI, storage, privacy

| ID | Threat | S | Mitigation | Residual | By | Test |
|----|--------|---|-----------|----------|----|------|
| T40 | Local IPC reachable from web pages or other users (OpenCode CVE-2026-22812/22813, Claude IDE ws CVE-2025-52882, rmcp DNS rebinding CVE-2026-42559 [O2, CC19, M1]) | S/E | Default: engine is a child on inherited pipes, no listener. If a daemon: UDS in 0700 dir + `SO_PEERCRED`/`LOCAL_PEERCRED`, Windows named pipe with owner-only DACL + `FILE_FLAG_FIRST_PIPE_INSTANCE` + client PID check; per-launch 256-bit token. **Never TCP/HTTP** | Same-user malware | M1 | Port scan shows no listener; second-user connect fails; sandboxed child connect fails |
| T41 | Sandboxed child reaches engine socket/pipe or inherits fds | E | `CLOEXEC` everywhere, explicit handle list on Windows; engine socket masked in profile | — | M2 | fd enumeration inside child |
| T42 | Terminal escapes: OSC 52 clipboard, OSC 8 spoofed links, title, DECRQSS/answerback injection, private modes (claude-hud CVE-2026-47090 [R13]) | S/E | Emulator with closed allow-list: OSC 52 write off (read never), OSC 8 shows real target, no answerback beyond DA/CPR, kitty graphics file/shm transport off; **non-terminal text strips all escapes**; paste is bracketed and warns on newline/control | Emulator memory bugs | M2 | esctest-derived negatives; fuzz emulator with ASAN |
| T43 | Custom URL scheme / deep link installs MCP or runs prompt (CVE-2025-54133, -64106 [CU10]) | S | Deep links only open a native review screen; never carry commands; full argv shown | User confirms | M5 | Deep-link corpus |
| T44 | Secrets in logs, crash dumps, transcripts, exports, process list | I | `Secret<T>` without Debug/Serialize; redaction at sink (structured keys + known-value + entropy); never in argv; core dumps off; crash upload opt-in and scrubbed; export preview | Novel secret formats in model text | M1 | Canary secret through every tool, error path and crash; grep all outputs |
| T45 | Credential theft at rest / in child env (shared-machine, TS bearer) | I | OS store only (Keychain / Credential Manager / Secret Service), no plaintext fallback: without a store, memory-only; child env built from allow-list; sandbox denies keychain Mach services and `security`; proxy injects real credential for sentinel | Harnesses with their own plaintext files | M1 | `env`/`ps e`/`/proc/*/environ` probe from a child |
| T46 | Insecure temp files (CVE-2026-46406 [CC20]) | T/I | Per-user 0700 dir, `O_EXCL\|O_NOFOLLOW` | — | M1 | Pre-planted symlink |
| T47 | Opening agent-made files executes them (Claude Desktop GHSA-v234 [CC21]) | E | "Open" uses an allow-list of inert types; preserve quarantine/provenance and Mark-of-the-Web | New OS file type | M2 | Type list per OS |
| T48 | Audit tampering / repudiation | R | Append-only, hash-chained log outside writable roots; approvals record rendered text hash | Local user can delete | M2 | Chain verification test |
| T49 | Plugin dialogs and TCC inheritance: agent commands inherit Axiom's macOS TCC grants (Cursor CVE-2024-45599) | E | Hardened runtime, no `allow-dyld`/JIT entitlements, no `disable-library-validation`; spawn tools with `responsibility_spawnattrs_setdisclaim` **[I: private API, verify]** | User grants Axiom Full Disk Access | M5 | `codesign -d --entitlements`; TCC probe |

## 6. Security invariants

Each is stated so that a type, a module boundary or a test can enforce it.

- **SI-1** `caps(child) ⊆ caps(parent) ∩ caps(workspace) ∩ user policy` for every spawn: subagent, harness, MCP server, script, shell.
- **SI-2** Only the engine mints or widens authority, and only from an authenticated UI gesture on an engine-rendered request. No text, message, tool result or file is ever an approval.
- **SI-3** Provider, OAuth and MCP secrets never enter model context, transcripts, logs, argv or a child's environment. The only exception is a user-configured secret delivered to the single MCP server/harness that owns it, recorded as such.
- **SI-4** A credential is bound to an origin and is never sent elsewhere, including on redirect.
- **SI-5** Workspace-sourced configuration can only narrow authority; it can never name an executable, endpoint, environment variable or permission mode that takes effect without user-scope consent bound to its content hash.
- **SI-6** Nothing from a workspace is executed, and no network request is caused by it, before trust.
- **SI-7** Every process that can run model-influenced code is spawned by one module (`spawn`), which requires a `SandboxProfile` value; there is no second spawn path. `Unsandboxed` is a distinct type needing a grant token.
- **SI-8** Sandbox construction failure is an error, never a downgrade.
- **SI-9** Engine file I/O on workspace paths is handle-relative (no check-then-open by string). Canonicalisation failure denies.
- **SI-10** Writable roots, cwd roots and protected paths come from the session record, never from model output.
- **SI-11** Protected paths are deny-write whether or not they exist, for every agent and harness, in the workspace and in HOME.
- **SI-12** Axiom's own state, logs, policy, sockets and binaries lie outside every writable root and are unreadable in the sandbox.
- **SI-13** All egress from sandboxes and engine fetch tools passes one policy object that decides on host, port and *resolved* address; no resolver is available inside the sandbox.
- **SI-14** The UI process performs no network I/O and spawns nothing except the engine.
- **SI-15** Untrusted text reaches a renderer only as `Untrusted<T>`; approvals use a renderer that escapes every non-printing code point and never truncates the decisive part.
- **SI-16** Auto-allow requires a complete parse into a closed grammar; anything else prompts. `deny` always beats `allow`, at any scope.
- **SI-17** Labels (origin, taint) are attached at ingestion and only ever union; summarisation, memory and agent messages preserve them.
- **SI-18** A tainted context holding private data cannot use an exfil-capable tool without fresh approval.
- **SI-19** Tool grants are keyed on (principal, tool identity, schema hash, argument shape, workspace); any change invalidates.
- **SI-20** No listener on any IP address, except on `127.0.0.1` while a sign-in waits for its person (decision 0013). Local IPC authenticates the peer's OS identity and a per-launch token.
- **SI-21** Workspace id is part of every storage key; retrieval APIs take a workspace capability, not an id.
- **SI-22** Engine git and tool binaries are absolute, resolved outside writable roots; git runs only through the hardened wrapper.
- **SI-23** Every privileged action is written to the append-only audit log before it happens, with the originating agent chain.
- **SI-24** Budgets (depth, fan-out, tokens, time, money) are inherited by intersection; cancelling a parent kills the whole tree.
- **SI-25** Installed code (extensions, skills, MCP packages, updates) is identified by content hash and signature, not by name, tag or branch.
- **SI-26** Containment owner (Axiom / harness / none) is exactly one value per session, persisted, displayed, and part of the audit record.

## 7. Incident patterns that shaped the invariants

| Pattern | Instances | Invariant |
|---------|-----------|-----------|
| Shell string allow-list bypass | ≥ 12 advisories across Claude Code, Cursor, Zed [A] | SI-7, SI-16 |
| Config is code | CVE-2025-54135/54136/59536, CVE-2026-21852/25725/33068/48124 [A] | SI-5, SI-6, SI-11 |
| Path string ≠ file | CVE-2026-39861/50549/88968/103435/27967 [A] | SI-9 |
| Model chooses the boundary | CVE-2025-59532, CVE-2026-50548 [A] | SI-10 |
| Empty policy treated as "no policy" | sandbox-runtime CVE-2025-66479 [A] | SI-8 |
| Host helper trusts agent output | Pillar series; CVE-2026-73217 [A, R] | SI-11, SI-22 |
| Localhost is not a boundary | CVE-2025-49596/52882, CVE-2026-22812/22813/42559 [A] | SI-20 |
| Pre-approved host as exfil | CVE-2025-55284, CVE-2026-54316 [A] | SI-13, SI-18 |
| Reference ≠ content | CVE-2026-86063, MCPoison [A] | SI-19, SI-25 |

## 8. Out of scope and accepted

Same-user malware already running; kernel and hypervisor 0-days (shared-kernel sandboxes; a VM backend is the answer for
users who need it); malicious provider; physical access; model correctness. Native Windows has weaker egress containment
than macOS/Linux until a BaseContainer-class backend is verified (sandboxing.md §4); the UI must say so.

## 9. Sources

Advisory ids resolve at `https://github.com/<repo>/security/advisories/<GHSA>`. Read via the GitHub API on 2026-11-24.

- Claude Code (`anthropics/claude-code`): CC1 GHSA-5xjx-9qx6-jcrv · CC2 GHSA-5j29-h97v-84ch · CC3 GHSA-4fgq-fpq9-mr3g · CC4 GHSA-5hhx-v7f6-x7gv · CC5 GHSA-jh7p-qr78-84p7 · CC6 GHSA-mmgp-wc2j-qcv7 · CC7 GHSA-q5hj-mxqh-vv77 · CC8 GHSA-pmw4-pwvc-3hx2 · CC9 GHSA-4p2j-qj3c-6x84 · CC10 GHSA-vp62-r36r-9xqp · CC11 GHSA-ff64-7w26-62rf · CC12 GHSA-xq4m-mc3c-vvg3 (and the seven siblings named in T16) · CC13 GHSA-x5gv-jw7f-j6xj · CC14 GHSA-fg94-h982-f3mm · CC15 GHSA-vhw5-3g5m-8ggf · CC16 GHSA-pq7j-f95f-qfcv · CC17 GHSA-5cwg-9f6j-9jvx · CC18 GHSA-7835-87q9-rgvv · CC19 GHSA-9f65-56v6-gxw7 · CC20 GHSA-4vp2-6q8c-pvq2 · CC21 GHSA-v234-4jrq-mgg6
- Cursor (`cursor/cursor`): CU1 GHSA-pc9j-3qc2-95wv · CU2 GHSA-3v8f-48vw-3mjx · CU3 GHSA-8pcm-8jpx-hv8r · CU4 GHSA-p9g2-cr55-cw9c · CU5 GHSA-24mc-g4xr-4395 · CU6 GHSA-3p48-7v9f-v5cw · CU7 GHSA-82wg-qcm4-fp2w · CU8 GHSA-v4xv-rqh3-w9mc · CU9 GHSA-43wj-mwcc-x93p, GHSA-xw2x-252g-97w2 · CU10 GHSA-r22h-5wp2-2wfv, GHSA-4575-fh42-7848
- Codex (`openai/codex`): CX1 GHSA-w5fx-fh39-j5rw. sandbox-runtime (`anthropic-experimental/sandbox-runtime`): GHSA-9gqj-5w7c-vx47. goose (`aaif-goose/goose`): G1 GHSA-r5pp-p5r8-466r. OpenCode (`anomalyco/opencode`): O1 GHSA-632h-h47v-g4x4 · O2 GHSA-vxw4-wv6m-9hhh, GHSA-c83v-7274-4vgp
- Zed (`zed-industries/zed`): Z1 GHSA-786m-x2vc-5235 · Z2 GHSA-fj2r-rmw6-h222 · Z3 GHSA-29cp-2hmh-hcxj · Z4 GHSA-v385-xh3h-rrfr, GHSA-59p4-3mhm-qm3r · Z5 GHSA-f2g4-87h6-4pxq · Z6 GHSA-c3g6-c3ff-69cg, GHSA-rqq3-p6x4-q866, GHSA-c99f-97vf-4h5h
- MCP: M1 `modelcontextprotocol/rust-sdk` GHSA-9g45-5xwm-f3wc, GHSA-89vp-x53w-74fx, GHSA-c9xm-49cp-xcr9, GHSA-5p52-q7pq-9fq2, GHSA-c9f9-cp8h-jcj5 · M2 `…/inspector` GHSA-q978-78c7-phqw, GHSA-pp85-586h-2m95, GHSA-rwv7-mh2c-ff2p, GHSA-7f8r-222p-6f5g · M3 `…/typescript-sdk` GHSA-6qxp-vccf-f47h. Spec revision 2026-07-28 (dynamic client registration deprecated): https://modelcontextprotocol.io/docs/tutorials/security/security_best_practices
- ACP: https://agentclientprotocol.com (docs/protocol/v1/tool-calls: "Agents run tool calls themselves"; permission requests are *MAY*) [S]
- P1 Pillar, *The Week of Sandbox Escapes*: https://www.pillar.security/blog/the-week-of-sandbox-escapes and https://www.pillar.security/blog/escaping-antigravitys-allow-default-seatbelt
- D1 https://code.claude.com/docs/en/sandboxing
- R1 s1ngularity: https://snyk.io/blog/weaponizing-ai-coding-agents-for-malware-in-the-nx-malicious-package · R2 QUIETVAULT: https://checkmarx.com/zero-post/when-malware-borrows-your-ai-coding-agent-to-find-your-secrets/ · R3 https://research.checkpoint.com/2026/rce-and-api-token-exfiltration-through-claude-code-project-files-cve-2025-59536/ · R4 https://arxiv.org/html/2509.10540v1 · R5 https://www.theregister.com/2025/10/09/github_copilot_chat_vulnerability/ · R6 https://darkreading.com/vulnerabilities-threats/flaws-google-apk-python-agent-to-agent-attack · R7 https://simonwillison.net/2025/Sep/24/cross-agent-privilege-escalation/ · R8 https://blog.trailofbits.com/2025/04/29/deceiving-users-with-ansi-terminal-codes-in-mcp/ · R9 https://www.reversinglabs.com/blog/postmark-mcp-attack-takeaways · R10 https://thehackernews.com/2026/02/researchers-find-341-malicious-clawhub.html · R11 https://cline.bot/blog/post-mortem-unauthorized-cline-cli-npm · R12 https://workos.com/blog/npm-worm-coding-agent-config-credentials.md · R13 https://nvd.nist.gov/vuln/detail/cve-2026-47090
- R items were read as search-result excerpts, not full pages; P1, D1, A items were read directly. A reported September 2026 Codex escape (BleepingComputer) could not be fetched and is **unverified**.
