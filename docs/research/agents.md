# LLM agent runtime infrastructure
> Researched 2026-11-11. Status of each claim: sourced unless listed as unverified.

**Research access:** Web search and web fetch worked, but some individual fetches failed on cross-origin redirects, including OpenAI function-calling, OpenAI security, and Gemini function-calling pages. No live provider or agent was run; compatibility is not experimentally demonstrated here. Repository `main` versions below are observed source-tree versions, not asserted published releases. Recommendations are design judgments, not upstream guarantees.

## Summary and recommendations
- **Adopt — in-house Rust agent loop:** retain control of tools, policy, persistence and visual-intelligence integration; learn from existing harnesses rather than link their internals.
- **Adopt — native OpenAI Responses, Anthropic Messages and Gemini adapters:** preserve opaque provider blocks and capabilities; do not reduce everything to chat text.
- **Adapt — OpenAI Chat Completions:** support the broad compatible-server ecosystem with per-server/model capability profiles and tests.
- **Adapt — Ollama, llama.cpp, LM Studio, MLX-LM and vLLM:** connect externally; never assume OpenAI wire compatibility means semantic parity.
- **Adopt — MCP client and opt-in server:** reuse one centrally authorized tool registry; start with local stdio and explicitly secure network exposure.
- **Adopt — ACP client:** host compatible external agents as separate processes; it complements rather than replaces MCP.
- **Defer — A2A:** consider only for remote agent-to-agent delegation; it is not an editor-agent or tool protocol substitute.
- **Adapt — Codex, Goose, Gemini CLI, OpenCode, Aider, Cline/Roo, Continue, OpenHands and Zed:** integrate via documented process/protocol boundaries or study their designs; do not assume stable in-process APIs.
- **Adopt — scoped permissions, private checkpoints and per-agent worktrees:** require explicit review for destructive actions and merge conflicts.
- **Adopt — event-sourced local sessions:** keep provider-native replay data, separate secrets, checkpoint links and explicit resume semantics.

## Findings

### OpenAI Chat Completions and Responses
- **Purpose:** Both expose model generation and tool/function calling; Responses is a distinct API, not merely a new name for Chat Completions. [Responses API](https://platform.openai.com/docs/api-reference/responses/create), [Chat API](https://platform.openai.com/docs/api-reference/chat/create).
- **Streaming:** Responses emits named events, including `response.output_text.delta`, `response.function_call_arguments.delta` and completion/failure events; Chat streams incremental `choices[].delta`. Reassemble arguments before executing. [Responses streaming](https://platform.openai.com/docs/api-reference/responses-streaming), [Chat streaming](https://platform.openai.com/docs/api-reference/chat-streaming).
- **Tools:** model produces function-call name, call identifier and JSON arguments; application executes and returns correlated output. More than one tool call may be requested. [Function calling](https://developers.openai.com/api/docs/guides/function-calling).
- **Structured outputs:** JSON Schema structured output and strict function schemas are separate from unconstrained JSON mode; supported schema features and model support must be checked. [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs).
- **Maturity/licence/platform:** hosted proprietary API, terms and account/model availability apply; HTTPS is portable, not an open-source runtime. [API reference](https://platform.openai.com/docs/api-reference/introduction).
- **Complexity:** medium–high: separate request/stream parsers, error handling, usage, cancellation, content and state mechanisms.
- **Alternative/verdict:** Chat is valuable for compatibility; implement Responses natively rather than discarding Responses-specific output items.

### Anthropic Messages
- **Purpose:** Messages returns ordered typed content blocks, including `tool_use`; tool results reference the tool-use ID. [Tool workflow](https://platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works).
- **Streaming:** `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`; deltas include partial tool-input JSON and thinking/signature forms. [Streaming](https://platform.claude.com/docs/en/build-with-claude/streaming).
- **Thinking:** preserve thinking and redacted thinking blocks, their signatures and ordering exactly when required in a tool-use continuation; never reconstruct them from displayed text. Current thinking workflows have evolved beyond the legacy extended-thinking page. [Thinking tool workflows](https://platform.claude.com/docs/en/build-with-claude/thinking-tool-workflows), [legacy extended thinking](https://platform.claude.com/docs/en/build-with-claude/extended-thinking).
- **Caching:** prompt caching has explicit cache controls and provider-specific accounting/invalidation; it is not a generic cross-provider flag. [Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching).
- **Maturity/licence/platform:** commercial API with model-specific features and service terms; any HTTPS platform. [Messages](https://platform.claude.com/docs/en/api/messages/create).
- **Complexity:** high: ordered blocks, signatures, partial JSON and cache-boundary preservation.
- **Alternative/verdict:** native adapter; lossy generic chat conversion is unsuitable for replay.

### Google Gemini API
- **Purpose:** Gemini exposes structured content/parts and function calling; calls and function responses must be associated using the provider's current schema. [Gemini function calling](https://ai.google.dev/gemini-api/docs/function-calling).
- **Thinking:** thought signatures and model-specific requirements must be preserved rather than inferred from text. [Thought signatures](https://ai.google.dev/gemini-api/docs/thought-signatures).
- **Maturity/licence/platform:** hosted provider API; model, account and regional availability are provider-dependent. [Gemini API](https://ai.google.dev/gemini-api/docs).
- **Integration:** native adapter, stream parser and provider-specific metadata; OpenAI-compatible endpoint is a separately documented alternative, not proof of feature equivalence. [OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai).
- **Verdict:** adapt native API; exact streaming shapes and current supported model list need fresh endpoint testing.

### Ollama
- **Purpose:** local model serving with native `/api/chat` and OpenAI-compatible endpoints. [API](https://docs.ollama.com/api/introduction), [compatibility](https://docs.ollama.com/api/openai-compatibility).
- **Tool use:** documented for supporting models, including parallel tool examples; capability depends on model/template, not just server. [Tool calling](https://docs.ollama.com/capabilities/tool-calling).
- **Maturity/licence/platform:** open-source server; [repository licence](https://github.com/ollama/ollama/blob/main/LICENSE); model licences are separate. Platform/GPU support depends on installation. [Download](https://ollama.com/download).
- **Known issue:** a reported model/tool-call completion failure illustrates that capability must be tested per model, not presumed universal. [Issue #12187](https://github.com/ollama/ollama/issues/12187).
- **Complexity/verdict:** medium; implement native adapter or compatibility profile, and expose user-overridable context settings. [Context length](https://docs.ollama.com/context-length).

### llama.cpp server
- **Purpose:** `llama-server` exposes an OpenAI-compatible HTTP server and local inference. [Server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).
- **Maturity/licence/platform:** project licence is [MIT](https://github.com/ggml-org/llama.cpp/blob/master/LICENSE); CPU and hardware backends have distinct build/platform requirements. [Repository](https://github.com/ggml-org/llama.cpp).
- **Complexity:** model templates, configured context, server version and tool parsing affect real behaviour; do not infer them from endpoint name. [Server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).
- **Alternative/verdict:** connect as external compatible server; validate its installed version and model.

### LM Studio
- **Purpose:** local desktop model manager and server with OpenAI-compatible API; exposes model/tool functionality through its documented endpoints. [OpenAI compatibility](https://lmstudio.ai/docs/developer/openai-compat), [tool use](https://lmstudio.ai/docs/developer/openai-compat/tools).
- **Maturity/licence/platform:** proprietary application; consult current [download/system requirements](https://lmstudio.ai/download) and terms rather than assuming redistribution rights.
- **Complexity/verdict:** connect externally, not embed; test model loading, tool streaming and user-configured context independently.

### MLX-LM server
- **Purpose:** MLX-based local inference with a server and OpenAI-style examples. [Repository](https://github.com/ml-explore/mlx-lm), [tool example](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/examples/openai_tool_use.py).
- **Licence/platform:** [MIT](https://github.com/ml-explore/mlx-lm/blob/main/LICENSE); MLX targets Apple silicon, so it is not Axiom's cross-platform baseline. [MLX](https://github.com/ml-explore/mlx).
- **Known limitation:** feature support must be checked against version; unsupported frequency and presence penalties were raised upstream. [Issue #132](https://github.com/ml-explore/mlx-lm/issues/132).
- **Verdict:** optional external macOS provider, not bundled default.

### vLLM
- **Purpose:** high-throughput serving with OpenAI-compatible endpoints; tool support has parser/model/configuration dependencies. [Server](https://docs.vllm.ai/en/latest/serving/openai_compatible_server/), [tool calling](https://docs.vllm.ai/en/latest/features/tool_calling/).
- **Licence/platform:** [Apache-2.0](https://github.com/vllm-project/vllm/blob/main/LICENSE); hardware and installation support are platform-specific. [Installation](https://docs.vllm.ai/en/latest/getting_started/installation/).
- **Complexity/verdict:** external server for capable machines/remote hosts; configure model-specific parser and test before advertising tools.

### Compatible-server interoperability
- **Documented distinction:** Ollama publishes an explicit compatibility list, while vLLM documents model-specific tool parsers; OpenAI naming therefore is not proof of full API coverage. [Ollama](https://docs.ollama.com/api/openai-compatibility), [vLLM](https://docs.vllm.ai/en/latest/features/tool_calling/).
- **Tool-call streaming:** collect fragments keyed by choice/index/call ID; tolerate absent or late metadata, but reject invalid final JSON; specific server behaviours require tests.
- **Parallel calls:** distinguish model emission from server parsing and runtime concurrent execution; do not claim support until tested for that exact pair.
- **Context discovery:** model maximum, server-configured capacity and effective remaining budget are different; provide an explicit override and conservative unknown state.
- **Failure strategy:** unsupported capability → clear configuration error or explicit fallback; never silently discard tool calls, reasoning data or media.
- **Platform/licence:** determined separately by server and model; a network protocol conveys no model redistribution rights.
- **Verdict:** versioned capability matrix and captured SSE fixtures, not one `openai_compatible = true` boolean.

### Rust provider and transport crates
- **`reqwest` + `eventsource-stream`:** HTTP/SSE building blocks; maximum control, but Axiom owns provider schemas, retries and fixtures. [reqwest](https://github.com/seanmonstar/reqwest), [eventsource-stream](https://github.com/jpopesculian/eventsource-stream).
- **`async-openai`:** community OpenAI-oriented typed client; evaluate version coverage before relying on it for Responses and raw passthrough. [Repository](https://github.com/64bit/async-openai).
- **`genai`:** multi-provider Rust client worth testing for provider coverage and metadata fidelity, not an automatic round-trip guarantee. [Repository](https://github.com/jeremychone/rust-genai).
- **`rig`:** agent/application framework with provider abstractions; compare its event and tool types to Axiom requirements before coupling the loop. [Repository](https://github.com/0xPlaygrounds/rig).
- **`llm`:** name is ambiguous across projects/crates; do not select without exact crate identity and current maintenance audit.
- **Licences/maturity:** dependency version, SPDX expression and maintenance must be verified per selected crate and transitive feature set; no licence is inferred from a repository name.
- **Verdict:** adopt HTTP/SSE primitives behind Axiom's interface; prototype client crates behind adapters, avoid making a framework's message enum the persistent schema.

### Model Context Protocol and rmcp
- **Current spec:** [2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28) is labelled latest on the specification page on the research date; JSON-RPC protocol with negotiated capabilities.
- **Primitives:** servers expose [tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools), [resources](https://modelcontextprotocol.io/specification/2026-07-28/server/resources) and [prompts](https://modelcontextprotocol.io/specification/2026-07-28/server/prompts); clients can expose other negotiated features.
- **Transports:** stdio for locally launched processes and [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http) for remote or HTTP connections; version negotiation and lifecycle matter. [Transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports).
- **Authorization:** HTTP authorization follows MCP's OAuth-based requirements; local stdio credentials come from the environment and process launch context, not the HTTP flow. [Authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization).
- **SDK:** official Rust `rmcp` workspace on `main` declares version **3.9.0**, Rust **1.88**, edition 2024 and **Apache-2.0**; verify released crate and supported spec before pinning. [Manifest](https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/Cargo.toml).
- **Server benefit:** external MCP-capable agents can discover Axiom UI/source tools without embedding Axiom's Rust code; specific client compatibility remains an integration test. [MCP overview](https://modelcontextprotocol.io/specification/2026-07-28).
- **Server cost:** process identity, workspace selection, origin validation, authorization, side-effect approvals, lifecycle and prompt injection become product responsibilities. [Security](https://modelcontextprotocol.io/specification/2026-07-28/basic/security_best_practices).
- **Client benefit:** reuse external tool ecosystems and resources; **cost:** arbitrary server code/network endpoints add trust and supply-chain boundaries.
- **Verdict:** adopt both roles via adapters to a single registry; network server off by default; do not trust annotations as authorization.

### Agent Client Protocol (ACP)
- **Purpose:** standardizes interaction between a code editor/client and a separate coding agent, including session lifecycle, updates and permission interactions; it is not a provider wire API. [Introduction](https://agentclientprotocol.com/get-started/introduction), [architecture](https://agentclientprotocol.com/get-started/architecture).
- **Adoption:** Gemini CLI documents [ACP mode](https://geminicli.com/docs/cli/acp-mode/); consult the [ACP registry](https://agentclientprotocol.com/get-started/agents) and test actual launch commands/version capabilities before promising support for any agent.
- **Rust SDK:** workspace `agent-client-protocol` **5.2.0**, schema **1.17.0**, Rust **1.88.0**, **Apache-2.0** on `main`. [Manifest](https://raw.githubusercontent.com/agentclientprotocol/rust-sdk/main/Cargo.toml).
- **Platform:** protocol is portable; each external agent has independent executable, authentication and OS requirements.
- **Complexity:** medium–high: subprocess lifecycle, session resume, terminal/file callbacks, permission prompts, cancellation and protocol versioning.
- **Alternative/verdict:** adopt ACP client for UI-hosted agents; use MCP for access to Axiom tools, and A2A for different delegation concerns.

### A2A
- **Purpose:** Agent2Agent addresses communication/delegation between agents, rather than editor-to-agent hosting or a standardized tool catalogue. [A2A specification](https://a2a-protocol.org/latest/specification/).
- **Licence/maturity/platform:** consult the exact [A2A repository](https://github.com/a2aproject/A2A) and release for applicable licence and version; network transport introduces authentication and deployment requirements.
- **Verdict:** defer until Axiom needs external agent-to-agent tasks; an adapter is plausible, not proven necessary.

### OpenAI Codex CLI
- **Architecture:** Rust coding agent repository; app-server integration is a documented boundary. [Repository](https://github.com/openai/codex), [app server](https://developers.openai.com/codex/app-server/).
- **Licence:** [Apache-2.0](https://raw.githubusercontent.com/openai/codex/main/LICENSE); shipping service and model access have separate terms.
- **Integration:** prefer app-server/process or a confirmed ACP adapter; Rust core reuse is not evidence of a stable public library contract. Platform sandbox behaviour differs by OS. [Security](https://developers.openai.com/codex/security/).
- **Verdict:** integrate optionally and study; do not build Axiom around internal core crate APIs.

### Claude Code and Agent SDK
- **Architecture:** proprietary Claude Code executable/service, separate from the separately licensed client SDK; [Agent SDK overview](https://platform.claude.com/docs/en/agent-sdk/overview), [SDK Python licence](https://github.com/anthropics/claude-agent-sdk-python/blob/main/LICENSE).
- **Integration:** SDK or external process where permitted; don't infer rights to redistribute or embed Claude Code from the SDK licence. [Claude Code](https://code.claude.com/docs/en/overview).
- **Verdict:** optional external integration and permission-model reference; do not vendor the proprietary harness.

### Gemini CLI
- **Architecture/licence:** open CLI repository, [Apache-2.0](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/LICENSE); docs include ACP, trust, worktrees and checkpointing. [Documentation](https://geminicli.com/docs/).
- **Important current restriction:** official site says unpaid tier and Google One users were moved to Antigravity CLI on **2026-06-18**; availability is not universal. [Checkpoint page banner](https://geminicli.com/docs/cli/checkpointing/).
- **Verdict:** use ACP where available; learn from checkpoint and trust implementation rather than embed its TypeScript runtime.

### Goose
- **Architecture/licence:** Block's open-source agent, with Rust implementation; [Apache-2.0 licence](https://raw.githubusercontent.com/block/goose/main/LICENSE), [repository](https://github.com/block/goose).
- **Integration:** candidate for process/protocol integration and design study; stable embeddable Rust API is not established by this research.
- **Verdict:** adapt patterns, avoid hard runtime dependency without API audit.

### OpenCode
- **Architecture/licence:** open-source agent project, [MIT licence](https://raw.githubusercontent.com/anomalyco/opencode/dev/LICENSE); [repository](https://github.com/anomalyco/opencode).
- **Integration:** separate process/API or ACP only after testing its documented version; inspect licences for individual dependencies.
- **Verdict:** optional external harness; don't assume Rust in-process embedding.

### Aider
- **Architecture/licence:** Python terminal pair-programmer; [Apache-2.0](https://raw.githubusercontent.com/Aider-AI/aider/main/LICENSE.txt), [repository map](https://aider.chat/docs/repomap.html).
- **Integration:** external process or learn editing/repomap algorithms; Python runtime and UX coupling make direct Rust embedding a separate engineering project.
- **Verdict:** strong reference for measured edit-format experiments.

### Cline and Roo Code
- **Architecture/licence:** agent projects with editor-oriented integrations; Cline [Apache-2.0](https://raw.githubusercontent.com/cline/cline/main/LICENSE), Roo [Apache-2.0](https://raw.githubusercontent.com/RooCodeInc/Roo-Code/main/LICENSE).
- **Integration:** inspect current extension/runtime APIs and dependency licences before packaging; their presence in a VS Code-derived editor does not prove Qt WebEngine compatibility. [Cline](https://github.com/cline/cline), [Roo](https://github.com/RooCodeInc/Roo-Code).
- **Verdict:** learn approvals/checkpoints; optional external integration only after extension-host validation.

### Continue
- **Architecture/licence:** open agent/editor project; root [Apache-2.0 licence](https://raw.githubusercontent.com/continuedev/continue/main/LICENSE), [repository](https://github.com/continuedev/continue).
- **Integration:** process or extension integration requires checking exact package and host compatibility; root licence is not a blanket dependency licence.
- **Verdict:** study/integrate conditionally, not a proven Rust library.

### OpenHands
- **Architecture/licence:** open agent platform; root [MIT licence](https://raw.githubusercontent.com/OpenHands/OpenHands/main/LICENSE), [repository](https://github.com/OpenHands/OpenHands).
- **Integration:** service/process architecture can be explored as optional remote execution; sandbox deployment and per-component licences need separate review.
- **Verdict:** learn from agent runtime and isolated execution; do not embed without a deployment and API audit.

### Zed agent
- **Architecture/licence:** Zed is a Rust editor and ACP originator; Zed has multiple licence files and component-specific licensing, including [GPL-3.0 text](https://github.com/zed-industries/zed/blob/main/LICENSE-GPL). Do not assign a single licence to every component without checking its manifest. [Zed repository](https://github.com/zed-industries/zed).
- **Integration:** ACP is the portable interface; Zed's agent internals are tied to an editor architecture.
- **Verdict:** adopt protocol concepts; avoid copying code before component-level licence review.

### Tool schemas, errors and edit formats
- **Schema:** give every tool a distinct purpose, clear parameter semantics, validation, bounded output and actionable errors; Anthropic recommends namespacing, filtering, pagination/range and evaluation on held-out tasks. [Writing tools](https://www.anthropic.com/research/writing-tools-for-agents).
- **Size:** Anthropic states Claude Code restricts tool responses to **25,000 tokens by default** in its article; this is an upstream example, not an appropriate universal Axiom limit. [Source](https://www.anthropic.com/research/writing-tools-for-agents).
- **Strictness:** JSON schema validation reduces ambiguity but cannot guarantee authorization, semantic correctness or safe side effects. [OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling).
- **Search/replace:** require exact unique match by default; report zero/multiple matches with helpful context, not a silent guess.
- **Unified diff:** support preview and atomic apply with precondition checks; do not assume a generated hunk is correct merely because it applies.
- **`apply_patch`:** a structured patch grammar is a distinct interface, not identical to arbitrary standard unified diff; parse explicitly. [Codex patch tool](https://github.com/openai/codex/tree/main/codex-rs/apply-patch).
- **Whole-file:** suitable for small/new files but high risk of deleting unrelated content; use version preconditions and review.
- **Measured, bounded evidence:** Aider's 89-task Python refactoring benchmark for `gpt-4-1106-preview` scored **20%** with its SEARCH/REPLACE format and **61%** with its adapted unified diff; its test is not a comprehensive correctness evaluation or evidence for all models. [Benchmark](https://aider.chat/docs/unified-diffs.html).
- **Verdict:** offer multiple formats behind one transactional edit engine; benchmark current models and record wrong-apply rate as well as task score.

### Context management and repository memory
- **Compaction:** summarize older context only with explicit durable summary/provenance and keep the full transcript outside model context. Anthropic documents a [compaction feature](https://platform.claude.com/docs/en/build-with-claude/compaction); provider-managed compaction is not automatically portable.
- **Subagents:** isolate task context and return bounded results; constrain each child's permissions and resource budget. [Gemini subagents](https://geminicli.com/docs/core/subagents/).
- **Repo map:** Aider documents a ranked repository map of important identifiers and signatures; [implementation](https://github.com/Aider-AI/aider/blob/main/aider/repomap.py), [design](https://aider.chat/docs/repomap.html).
- **Instruction conventions:** [AGENTS.md](https://agents.md/) is a convention for project instructions, not an authorization source; [Gemini's GEMINI.md](https://geminicli.com/docs/cli/gemini-md/) is a separate convention.
- **Memory:** persist scoped user/project facts as editable data with provenance, retention and trust labels; do not silently promote repository text to system authority. [Gemini auto memory](https://geminicli.com/docs/cli/auto-memory/).
- **Verdict:** separate immutable event log, compacted prompt view, repository index and user-approved memory.

### Permission and credential models
- **Codex:** approval and filesystem/network sandbox are separate axes; inspect current [Codex security documentation](https://developers.openai.com/codex/security/) for current policy names and defaults. Do not conflate user consent with OS enforcement.
- **Claude Code:** rules can use tool-specific patterns, such as `Bash(npm run *)`; deny/ask/allow and permission modes are documented, with important shell-pattern caveats. [Permissions](https://code.claude.com/docs/en/permissions).
- **Gemini:** trusted folders determine whether project configuration and tools may be trusted; folder trust is not an OS sandbox. [Trusted folders](https://geminicli.com/docs/cli/trusted-folders/).
- **Axiom proposal:** deny > ask > allow, rules on principal/tool/canonical resource/action/scope; no recursive directory or wildcard allow by default.
- **Default proposal:** untrusted workspace = ask or deny for writes, shell, network and external servers; no prompt available means deny, never implicit approval.
- **Approvals:** display exact command/patch, resolved paths, network targets and scope; one-shot/session/persistent grants must be explicit, revocable and auditable.
- **OS enforcement:** require real per-platform sandbox implementations or accurately disclose absence; prevent symlink/path and check-to-use races where feasible.
- **Credentials:** Rust [keyring](https://github.com/hwchen/keyring-rs) offers platform-specific credential-store integrations; select and test backend features on Linux, macOS and Windows. Do not log tokens in events or provider payload fixtures.
- **Verdict:** policy centrally enforced for native, MCP and ACP entry points; keychain unavailability must fail clearly or use an explicit alternative.

### Checkpoints, worktrees and concurrency
- **Documented precedent:** Gemini checkpoints save a shadow Git snapshot, conversation and pending tool call; feature is **disabled by default**. [Checkpointing](https://geminicli.com/docs/cli/checkpointing/).
- **Isolation:** Git [worktrees](https://git-scm.com/docs/git-worktree) enable distinct working directories attached to one repository; they do not sandbox network/processes or automatically incorporate the user's uncommitted files.
- **Axiom proposal:** create private checkpoint storage and a temporary index (`GIT_INDEX_FILE`) to capture the selected working-tree state; use `git write-tree` and `git commit-tree`, then maintain private refs. [git-write-tree](https://git-scm.com/docs/git-write-tree), [git-commit-tree](https://git-scm.com/docs/git-commit-tree).
- **Safety:** do not use ordinary `git stash` as an invisible implementation detail: it changes stash refs and can alter working state. [git-stash](https://git-scm.com/docs/git-stash).
- **Coverage:** specify tracked/untracked/ignored files, submodules, symlinks, modes, large files and secrets; a Git tree is not a universal complete machine snapshot.
- **Restore:** preview diff, verify live state against expected hashes, require approval, preserve newer user edits and refuse or merge on conflict; never silently reset a user's branch/index.
- **Parallelism:** one task/worktree per mutating agent; allocate CPU, memory, model-token and process budgets, cap concurrency and timeouts.
- **Merge:** compare each result with its common base; serialize integration, surface conflicts in UI and revalidate tests. A worktree alone does not resolve conflicts.
- **Alternative:** [Jujutsu](https://jj-vcs.github.io/jj/latest/) provides a different version-control model, but interoperability/installation and user workflow need evaluation; not a required dependency.
- **Verdict:** private Git snapshots plus isolated worktrees, with verified restore semantics; prototype before promising lossless rollback.

### Session persistence and resumability
- **Precedent:** Codex repository contains rollout/session machinery; exact stored schema is implementation-specific, not a universal JSONL standard. [Codex source](https://github.com/openai/codex/tree/main/codex-rs/core/src/rollout).
- **Design:** append-only versioned JSONL event records with session/turn/event IDs, monotonic sequence, timestamps, parent links and typed payloads; use separate blobs for large/binary data.
- **Replay:** persist complete provider-native messages and tool IDs alongside normalized projections, approval decisions, checkpoint IDs and visible compaction summaries.
- **Crash safety:** write tool intent before execution and durable result after; an interrupted side effect is `unknown`, not automatically safe to rerun.
- **Resumption:** distinguish UI reconstruction, model continuation and subprocess session reconnection; provider/server capabilities determine which are possible.
- **Privacy:** encrypt or restrict access to stored sensitive transcripts and thinking blocks; secret redaction must not corrupt the authoritative replay envelope.
- **Verdict:** adopt a versioned local event journal with explicit migration, retention and recovery tests.

## Architectural implications for Axiom

### Provider trait: proposed interface
```rust
trait Provider {
    fn capabilities(&self, model: &str) -> CapabilitySet;
    async fn stream(&self, request: ModelRequest) -> Result<EventStream, ProviderError>;
    async fn count_tokens(&self, request: &ModelRequest) -> Result<Option<TokenCount>, ProviderError>;
}
```
- `ModelRequest`: model, ordered `Message`s, tools, output schema, generation controls, cancellation and provider-specific options.
- `Message`: role, ordered `ContentBlock`s, provider origin, IDs and native envelope.
- `ContentBlock`: text, image/reference, tool call, tool result, thinking/redacted reasoning, refusal and unknown opaque block.
- Preserve raw native JSON, signed bytes/signatures, ordering, IDs and cache metadata; never send one provider's opaque block to another as if compatible.
- `StreamEvent`: block-start/delta/end, tool argument fragment, usage, finish, provider-error and unknown event; deliver ordered and backpressured.
- Provider-specific response/session IDs and continuation handles remain in an extension namespace; cross-provider switching is a lossy explicit operation.
- Capability values are `supported`, `unsupported` or `unknown`, with provenance, model/server/version and optional user override.
- Don't retry non-idempotent tool side effects on ambiguous failure; cancellation and partial stream completion are explicit states.

### Tool registry contract: proposed interface
- `ToolDefinition`: stable namespace/name/version, description, input/output schemas, side-effect class, capabilities and size limits.
- `ToolContext`: authenticated principal, workspace root, scoped grants, deadline, cancellation, budget, trace and idempotency key.
- `validate → resolve canonical targets → policy check → preview/checkpoint if needed → execute → audit → bounded result`.
- Result: typed content, error category, actionable message, truncation flag, continuation cursor and optional blob reference.
- Expose the same operations via native tool calls and MCP; ACP callbacks must go through the same policy gate.
- Tools are untrusted-data boundaries; instructions inside source files, web pages or results cannot grant permissions.

### Recommended sequencing
1. In-house loop, OpenAI Responses/Chat and Anthropic native adapters; deterministic simulated-provider tests first.
2. Native Gemini, local compatibility profiles, raw event capture and replay fixtures.
3. MCP stdio client and opt-in local server with workspace-bound approval.
4. ACP client, documented external-agent support matrix and OS-specific process controls.
5. Private checkpoint prototype; validate user-index preservation and conflict-safe restore.
6. Worktree workers with budgets, merge review, session journal and crash recovery.

## Risks
- API and protocol revisions outpace pinned crates; maintain versioned compatibility tests and regular dependency/licence review.
- Signed thinking or unknown blocks lost in normalization can make legitimate continuation fail or corrupt replay.
- Untrusted MCP servers, workspace instructions and tool results can attempt prompt injection or exfiltration.
- An approval UI without effective platform sandboxing is not a security boundary.
- Model/server claims of tools do not guarantee correct streaming, parallel execution or schemas.
- Checkpoints may capture secrets or miss ignored files/submodules; restore can overwrite work without compare-and-swap safeguards.
- Git worktrees isolate paths, not shared repository metadata, external services, databases, ports or CPU/memory.
- A local or remote model service can have separate weights, provider, data-retention and redistribution terms.
- ACP/MCP support varies by client release; published protocol support does not guarantee specific Axiom tools work.

## Unverified / needs experiment
- Fetch failures: OpenAI function-calling and Codex security URLs and Gemini function-calling URL redirected across origins; their exact current detailed content and defaults were not retrieved in full.
- Confirm current Codex approval policy names, exact defaults, sandbox modes and platform differences from accessible primary docs; do not assume a default from prior versions.
- Confirm Claude rule precedence and default modes for each deployment; test command globbing and chained commands with real versions.
- Determine crate-specific current versions, SPDX licences and maintenance for `async-openai`, `genai`, `rig`, `llm`, `reqwest`, `eventsource-stream` and `keyring`; no exhaustive crate audit was completed.
- Confirm exact MCP SDK 3.9.0 and ACP SDK 5.2.0 published releases and spec coverage; observed numbers are repository manifests only.
- Capture single/parallel/streamed tool-call fixtures and context-discovery behaviour for each named local server/model/version; no installations or live tests were performed.
- Verify MLX-LM current server tool support and LM Studio platform/version matrix; example existence does not prove all streaming paths.
- Verify Claude Code, Codex and Gemini CLI's current MCP connection commands and Axiom server interop; check ACP support or third-party adapter independently for each.
- Inspect Codex core crate stability and supported Rust embedding boundary, Goose SDK, OpenCode/Continue/Cline extension host dependencies, OpenHands API, and each Zed component licence.
- Confirm Cline checkpoint shadow-repository design and exclusions from primary documentation; do not assume it matches Gemini's implementation.
- Benchmark search/replace, unified diff, OpenAI `apply_patch` and whole-file on current models; cited Aider result does not compare all four.
- Test user-index preservation, untracked/ignored files, Windows paths, symlink races, submodules, private-ref garbage collection and interrupted restores before enabling snapshots.
- Test worktree initialization with dirty repositories, merges against changing bases, multiple agents writing shared resources and per-platform sandbox availability.
- Confirm exact JSONL rollout field schema, migration and native resume guarantees for each shipping harness; JSONL alone does not guarantee resumability.
- Decide practical session encryption, keychain backend fallback, privacy/retention and token-count estimates through threat modelling and platform tests.

## Sources
- [MCP specification, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28); [Rust SDK manifest](https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/Cargo.toml).
- [ACP documentation](https://agentclientprotocol.com/get-started/introduction); [Rust SDK manifest](https://raw.githubusercontent.com/agentclientprotocol/rust-sdk/main/Cargo.toml).
- [Anthropic tool design research](https://www.anthropic.com/research/writing-tools-for-agents); [streaming](https://platform.claude.com/docs/en/build-with-claude/streaming); [thinking workflows](https://platform.claude.com/docs/en/build-with-claude/thinking-tool-workflows).
- [OpenAI Responses](https://platform.openai.com/docs/api-reference/responses/create); [streaming events](https://platform.openai.com/docs/api-reference/responses-streaming); [Codex repository](https://github.com/openai/codex).
- [Gemini CLI checkpointing](https://geminicli.com/docs/cli/checkpointing/); [trusted folders](https://geminicli.com/docs/cli/trusted-folders/); [ACP mode](https://geminicli.com/docs/cli/acp-mode/).
- [Aider edit benchmark](https://aider.chat/docs/unified-diffs.html); [repository map](https://aider.chat/docs/repomap.html).
- [Git worktrees](https://git-scm.com/docs/git-worktree); [git commit-tree](https://git-scm.com/docs/git-commit-tree); [Jujutsu](https://jj-vcs.github.io/jj/latest/).
