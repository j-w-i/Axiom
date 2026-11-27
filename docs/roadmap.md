# Roadmap

Milestones are complete when every criterion is verified and recorded in `state.md`. `←` lists what must exist first.

`W-n` is item n of `product/worth-building.md`, `row n` one of `research/t3code-comparison.md`. They belong to the milestone they stand in (owner, 2026-11-25); a `D-n` there is written as its own decision before its code.

## M0 Reset and research
- [x] v1 archived, bundle restored and counted, no force-push; new orphan `main`
- [x] research reports with evidence grades; threat model with invariants; provider and harness matrices
- [x] architecture, alternatives measured (P1 wire, P2 language, P3 transcript), decisions 0001–0012
- [x] testing and performance standards; this roadmap
- [ ] owner has read the decisions that need them (0006, 0007)

## M1 Visual identity and foundations ← M0
- [x] several distinct concepts rendered and criticised; direction agreed with the owner
- [x] owner accepts the look (round 6, 2026-11-25)
- [ ] tokens, components, gallery; every control passes keyboard, semantics and contrast tests in both themes
- [ ] host + journal + wire + Dart client: a replayed session streams end to end through the real processes
- [ ] transcript: chunk-invariant incremental Markdown, virtualised; benchmarks recorded
- [ ] CI green on macOS, Linux, Windows; goldens on one OS; licences and advisories checked
- [ ] application shell on live host data (after the owner's yes), window chrome per OS

## M2 Native engine ← M1
Order: policy+spawn → Anthropic Messages codec → Chat codec (local servers) → loop → tools → Responses, Gemini → Bedrock.
- an agent on a local model (LM Studio) and one on a recorded cloud fixture complete the same five repository tasks; transcripts replay byte-identically
- tool set: read, edit, patch, write, list, glob, search, shell, background job, fetch; each with measured tokens and latency
- interrupt ≤ 100 ms to provider and process tree; kill -9 at any point loses no committed effect and repeats none (fault-injection test)
- sandbox: macOS and Linux escape suite passes; Windows states honestly that it is uncontained
- permissions: SI-1…SI-16 each have a test; compaction, branching, resume; `axiom` CLI can do everything the GUI can
- connections: several accounts per provider; secrets only in the OS store; none in logs, argv, env or journal (canary test)
- [x] talking and reading: earlier prompts with ↑ (W-1), find in the conversation (W-2), copy a message (W-3), quote (W-4), one table of commands and keys (W-5), compact now (W-6), settings but for notifications (W-14), CLI for scripts (W-16), open a folder from a terminal or drop it on the window (W-17)
- [x] branching as a person uses it (W-54, decision 0015)

## M3 Delegated harnesses ← M2 policy, spawn
- ACP client (v1, v2), Codex app-server, Claude stream-json, Copilot; each replayed from recorded frames and driven once by hand with the owner's consent
- capability matrix computed by `probe()`; absent features are absent in the UI
- trust gate before spawn; containment owner displayed; account, lane and policy displayed; quota only where reported
- Axiom's tools offered to harnesses by MCP with the caller's capabilities
- [x] programs: one of the person's own (W-8), path, arguments and environment (W-9), an agent's list of things to do (W-10), plan first (W-23), choosing a model (W-29), what a model can be told (W-30)
- [x] cost and provenance on the wire and in the journal, before any view (W-27's first step)

## M4 Advanced tooling ← M2
- repo map and symbol index; LSP; persistent terminals; Git and worktrees through the hardened wrapper; MCP client; browser
- deferred tools; programmatic calling decided by the 20-task evaluation (0012)
- agent benchmark (30 tasks × 3 harnesses × same model): success, tokens, time, published with raw data
- folders and files: jobs seen and stopped (W-11), ways to look at a diff (W-12), read a file (W-18), paths open it (W-19), find files and in files (W-20), open in an editor (W-21, D-3), pictures (W-22), folders with an identity (W-36) and commands (W-37), what uses the computer (W-38)
- git for a person (W-42 to W-46, D-1), terminals (W-47 to W-49), browser (W-50 to W-52, D-6)

## The switch: Axiom is no harness ← M4 (decision 0023; owner, 2026-11-27: before M5)
- [x] decision 0023, from `research/t3code-harness.md` and `research/harnesses-for-accounts.md`
- an account's model runs in DeepSeek Harness (ACP) by default; OpenCode (ACP) and Pi (RPC) can be chosen in Settings; each declares the providers it can run, its configuration in a folder of Axiom's own, the key only in its environment
- a harness that is not there is offered with its own install command, run by the person in their terminal; Axiom downloads nothing
- Axiom's MCP server gives every harness `agent` and `browser`, nothing a harness has
- the loop, its tools as a model's, compaction, branching of it and signing in with ChatGPT are gone; sessions of before are readable
- a tool-less client for short texts; a suggested commit message (W-43)
- each replayed from scripted programs through the real host; none of the three started for real without the owner's yes

## M5 Orchestration ← M3, M4 worktrees, the switch
- delegation across harnesses; exactly-once delivery under crash injection; child ⊆ parent under property test
- worktree per writer; conflict forecast; gated merge; needs-you queue; score; task graph with gates
- 10 agents on four harnesses for one hour: budgets met, no lost request
- rows 1 to 13 as far as they are left, with pin and order (W-7), remarks on several lines (W-13), how one is told, with its setting (W-15, and of W-14 notifications: there are none to set before), set aside by rule (W-33), nothing to do for you (W-34)
- agents ask for agents (W-24, not before K49 is settled), know and stop it (W-25), answers of a shape (W-26), runs that resume (W-53), ask beside (W-55), sessions in sessions (W-56), Axiom for other agents (W-57, D-7), compact while idle (W-58)

## M6 Usage, routing, extensions ← M5
- provenance on every figure; budgets; routing with explanations; import of other harnesses' files; extensions with capabilities and signed identity
- cost over time (W-27), limits beside the conversation (W-28, row 6), bring in earlier conversations (W-31, D-4), titles (W-32)

## M7 Hardening ← all
- external-style security audit, 8 h soak, crash matrix, three-OS visual audit, accessibility audit, signed releases and updates
- throw a session away (W-35, D-2), bounds on memory (W-39), first start (W-40), keys a person can change (W-41), upkeep (W-59)

## Dependency graph
```
M0 ─ M1 ─┬─ policy+spawn ─┬─ codecs ─ loop ─ tools ─┬─ M4 ─ switch ─┐
         │                └─ M3 adapters ───────────┼───────────────┼─ M5 ─ M6 ─ M7
         └─ transcript, deck, requests ─────────────┘               │
                                     worktrees (M4) ────────────────┘
```
