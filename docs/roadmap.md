# Roadmap

Milestones are complete when every criterion is verified and recorded in `state.md`. `←` lists what must exist first.

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

## M3 Delegated harnesses ← M2 policy, spawn
- ACP client (v1, v2), Codex app-server, Claude stream-json, Copilot; each replayed from recorded frames and driven once by hand with the owner's consent
- capability matrix computed by `probe()`; absent features are absent in the UI
- trust gate before spawn; containment owner displayed; account, lane and policy displayed; quota only where reported
- Axiom's tools offered to harnesses by MCP with the caller's capabilities

## M4 Advanced tooling ← M2
- repo map and symbol index; LSP; persistent terminals; Git and worktrees through the hardened wrapper; MCP client; browser
- deferred tools; programmatic calling decided by the 20-task evaluation (0012)
- agent benchmark (30 tasks × 3 harnesses × same model): success, tokens, time, published with raw data

## M5 Orchestration ← M3, M4 worktrees
- delegation across harnesses; exactly-once delivery under crash injection; child ⊆ parent under property test
- worktree per writer; conflict forecast; gated merge; needs-you queue; score; task graph with gates
- 10 agents on four harnesses for one hour: budgets met, no lost request

## M6 Usage, routing, extensions ← M5
- provenance on every figure; budgets; routing with explanations; import of other harnesses' files; extensions with capabilities and signed identity

## M7 Hardening ← all
- external-style security audit, 8 h soak, crash matrix, three-OS visual audit, accessibility audit, signed releases and updates

## Dependency graph
```
M0 ─ M1 ─┬─ policy+spawn ─┬─ codecs ─ loop ─ tools ─┬─ M4 ─┐
         │                └─ M3 adapters ───────────┼──────┼─ M5 ─ M6 ─ M7
         └─ transcript, deck, requests ─────────────┘      │
                                     worktrees (M4) ───────┘
```
