# Provider integration research

Date: 2026-11-24 (fetches 13:03 UTC). Revision 2: every claim is tagged. Replaces the recall-based draft.

## 0. Evidence legend and corpus
Tags: **[AI]** vercel/ai @8956ca5 (packages/*/src; read-only); **[LL]** BerriAI/litellm @731dce90; **[GN]** jeremychone/rust-genai @2a8a5b3; **[RG]** 0xPlaygrounds/rig @4d5fb5e; **[ORP]** OpenRouterTeam/ai-sdk-provider @7a235a4;
**[MD]** sst/models.dev @6e1fdc3 (repo files: README.md, AGENTS.md, providers/*/models/*.toml, each file carries sources and live-probe dates written by its authors); **[MDlive]** https://models.dev/api.json fetched
2026-11-24T13:03Z (326 providers, 10181 models); **[ORlive]** https://openrouter.ai/api/v1/models fetched 2026-11-24T13:03Z (473 models, unauthenticated); **[LLjson]** litellm model_prices_and_context_window.json fetched same time
(7158 keys); **[doc: url]** official page read through WebFetch on 2026-11-24; **[unverified: recall]** training memory, not backed by any source above. All clones are under `scratchpad/research/prov/` (never built or run). Raw
fetches: `or_models.json`, `modelsdev_api.json`, `litellm_prices.json` there. Caveat on dates: the world data is newer than my training (Claude 5.x "Fable/Mythos/Opus/Sonnet/Haiku", GPT-6.x, Gemini 4, DeepSeek V4, Kimi K3, GLM-5.x
appear in [MDlive] and [ORlive]). Where [doc] and [MD] disagree with recall, they win. Model ids below are examples from [MDlive]; do not hardcode. Vendor doc pages NOT read this pass (so related claims are [unverified: recall] or
taken from [AI]/[MD]): OpenAI streaming-events reference, Anthropic rate-limits page, Bedrock/Vertex/Azure docs, Groq, Mistral, xAI, Together, Cerebras, Fireworks, HF, Z.ai, Moonshot, MiniMax, Qwen, NIM, Vercel, Cloudflare docs.

## 1. Corrections versus the first draft (material)
- Network: not sandboxed; the owner's Wi-Fi was down. Cloning and fetches now done. models.dev licence is MIT (LICENSE file, (c) 2025 models.dev) [MD].
- Anthropic thinking: `thinking.type=enabled` (+budget_tokens) is a 400 on Claude 4.7 and later; use `thinking.type=adaptive` plus `output_config.effort`; `display` = omitted|summarized|updates; thinking always on for Fable/Mythos
  5.x and Opus 5.5 (disabled -> 400); Sonnet 5.5 uses `type: between_tools`; assistant prefill is a 400 on 4.6+ [doc: platform.claude.com/docs/en/api/errors].
- Anthropic forced tool use (`tool_choice any|tool`) is a 400 on Opus 5.5, Sonnet 5.5, Fable 5.1/5.2, Mythos 5.1/5.2 [doc: .../api/errors]. My earlier "forced-tool fallback for structured output" is invalid there; use strict tools
  + `auto` or `output_config.format` (page cites both) [doc].
- Anthropic thinking replay is stricter than I said: blocks are bound to system+tools+earlier messages ("prefix check"); editing tools/system mid-conversation can 400; models read only some other models' blocks and the API
  silently drops unreadable ones; some models' blocks are account-bound [doc: .../preserved-thinking]. So: never strip, keep history append-only, change tools via tool_addition/removal (see 2.1).
- Anthropic request examples now show `anthropic-version: 2026-10-21` [doc: tool-search page]; I wrote 2023-06-01. Whether old versions remain accepted: not checked.
- Anthropic errors also include 402 billing_error and 409 conflict_error; a 400 can mean spend limit; a tier-spend-cap 429 has no retry-after and will not clear by waiting [doc: .../api/errors].
- DeepSeek: my "never send reasoning back except tool loops" is wrong for V4. [AI deepseek/convert-to-deepseek-chat-messages.ts:331-369]: R1 must not receive prior reasoning; **V4 requires `reasoning_content` on every assistant
  turn (empty string back-fill)**. Docs: with `tools`, reasoning_content of all previous turns must be returned (omit -> 400); without tools it is ignored [doc: api-docs.deepseek.com/guides/thinking_mode].
- OpenAI caching changed: GPT-5.6+ has `prompt_cache_options{ttl 30m|12h, mode implicit|explicit, prewarm}`, per-block `prompt_cache_breakpoint`, max 4 writes, usage `cache_write_tokens`; older models keep `prompt_cache_retention
  in_memory|24h` [doc: developers.openai.com/api/docs/guides/prompt-caching]. My draft only had the old form.
- llama.cpp: `--jinja` is now default-on, `--parallel` default auto (-1), `--kv-unified` default on when slots are auto, `/slots` enabled by default, `--cache-ram` default 8192 MiB, router mode exists (see local doc). Draft had
  these wrong.
- Ollama: `prompt_eval_cached_count` exists in /api/chat stats, so cache hits are visible; default context is VRAM-tiered per docs/context-length.mdx while docs/faq.mdx still says 4096 (docs disagree) [src ollama@924cc28].
- OpenRouter does serve Responses and Messages surfaces: 381/473 models list /api/v1/responses and /api/v1/messages besides chat/completions [ORlive]. Draft called these "beta/unverified".
- Gemini: `thinking_level` is the control on Gemini 3+/4 (levels per model in 2.4); Interactions API (steps, `previous_interaction_id`, `store`) exists with signatures only on thought steps [doc: aistudio.google.com/docs/thinking,
  /docs/thought-signatures]. generateContent still exists [MD: 30 models with api.generate-content]. Gemini-3 functionCall replay without thoughtSignature gives HTTP 400, sentinel `skip_thought_signature_validator` [AI
  google/convert-to-google-messages.ts:49-54,815-826].
- Closed items from my "what needs checking first": see section 9.

## 2. Wire protocol families
### 2.1 Anthropic Messages `POST /v1/messages`
- Stream SSE events: message_start, content_block_start/delta/stop, message_delta, message_stop, ping, error (can follow HTTP 200, e.g. overloaded_error = 529 equivalent) [doc: build-with-claude/streaming].
- `signature_delta` arrives just before content_block_stop; with `display:"omitted"` the block gets an empty thinking_delta then one signature_delta [doc: streaming]. input_json_delta partial_json and fine-grained tool streaming
  opt-in per tool via `eager_input_streaming` [doc: streaming line 596]. Delta kinds also text_delta, citations_delta (the latter [AI anthropic] handles citations: map-anthropic-citation.ts).
- Block kinds seen in the adapter: redacted_thinking, compaction, server tool blocks with `caller` metadata [AI anthropic-language-model.ts:331,1694,1714,1766].
- Errors: 400 invalid_request, 401, 402 billing, 403, 404, 409, 413 request_too_large (128 MB Messages), 429, 500 api_error, 504 timeout_error, 529 overloaded; `request-id` header + `request_id` body field; SDK retries twice
  honoring retry-after [doc: api/errors].
- Reasoning controls (per [MDlive]/[MD] anthropic provider files): effort lists low|medium|high(+xhigh,max) by model; toggle on some; budget_tokens min 1024 on older. Adaptive thinking is default when `thinking` omitted on 5.x
  [doc: api/errors].
- Preserved thinking, supported models and switch matrix in [doc: build-with-claude/preserved-thinking]: blocks tied to prior prefix, model, account; `prefix_mismatch_behavior: drop_block` needs beta header
  `thinking-binding-controls-2026-08-01`; response `input_transformations` lists dropped blocks.
- Tool search: server tool `tool_search_tool_regex_20251119` / `_bm25_20251119`, `defer_loading:true` per tool (all definitions still sent every request; at least one tool non-deferred; deferred excluded from prefix so cache
  survives; a deferred tool cannot carry cache_control (400)); results arrive as server_tool_use + tool_search_tool_result with tool_reference blocks to be replayed unchanged; Bedrock: InvokeModel only, not Converse [doc:
  agents-and-tools/tool-use/tool-search-tool].
- Compaction: beta `compact-2026-09-04` with top-level `compaction` parameter (on demand: you request a summary block and send it first in messages) or threshold compaction (API compacts at a token trigger;
  `pause_after_compaction`) [doc: build-with-claude/compaction; param names also in AI anthropic-language-model.ts:673-1143]. Context editing (clear_tool_uses, clear_thinking) separate [doc compaction page; AI line 701].
- Mid-conversation system messages and per-message `output_config` effort changes, `tool_addition`/`tool_removal`: keep the cache on Messages API for listed models; flags per model in [MD providers/anthropic/models/*.toml]
  (`system_updates`, `effort_updates`, `tools.updates`); Sonnet 5 docs list as unsupported but API accepts (MD comment 2026-10-19). [LLjson] mirrors as `supports_mid_conversation_system`.
- Cache: explicit `cache_control`, ttl 5m|1h, max 4 breakpoints, min_tokens 512/1024/4096 by model, targets tools|system|messages [MD anthropic files]. Usage field names: [unverified: recall] cache_creation_input_tokens /
  cache_read_input_tokens (adapter file convert-anthropic-usage.ts exists [AI]).
- Chat-completions compat layer on api.anthropic.com/v1 ignores `strict`, takes only function tools, hoists system messages, rejects tool_search types (live probe 2026-10-26 noted in [MD anthropic/claude-sonnet-5.toml]); use
  native Messages.
- Rate-limit headers (`anthropic-ratelimit-*`, retry-after): [unverified: recall].

### 2.2 OpenAI Responses `POST /v1/responses`
- Stream events seen in [AI openai/src/responses/openai-responses-api.ts]: response.created / in_progress / completed / incomplete / failed, output_item.added/done, output_text.delta, output_text.annotation.added,
  refusal.delta/done, function_call_arguments.delta/done, reasoning_summary_part.added/done, reasoning_summary_text.delta, custom_tool_call_input.delta, code_interpreter_call_code.delta, image_generation_call.partial_image,
  apply_patch_call_operation_diff.delta.
- Items: message items carry `phase: commentary|final_answer` (must be preserved or GPT-5.5 may treat an update as final) [AI line 327; doc conversation-state]. reasoning items: `encrypted_content` via include
  `reasoning.encrypted_content`; returned by default as encrypted items in stateless use; replay every output item [doc: developers.openai.com/api/docs/guides/conversation-state; AI convert-to-openai-responses-input.ts:1406-1443].
  `reasoning.context="all_turns"` on persisted-reasoning models [doc].
- State: `store` default saves 30 days; `previous_response_id` chains but all prior input tokens still billed; `conversation` items exempt from TTL [doc conversation-state].
- Compaction: server-side `context_management` with `compact_threshold`; standalone `/responses/compact`; `compaction` item with `encrypted_content` must be replayed [doc conversation-state; AI openai-responses-api.ts:545-559,
  convert...:1457-1479].
- Caching (GPT-5.6+): min 1,024 tokens; `prompt_cache_options{ttl:30m|12h, mode, prewarm}`; `prompt_cache_breakpoint{mode:explicit,ttl}` on input content blocks; up to 4 writes per request;
  `usage.input_tokens_details.cached_tokens` and `cache_write_tokens(+_details.ttl_30m_tokens/ttl_12h_tokens)`. Earlier models: `prompt_cache_key` routing hint, `prompt_cache_retention in_memory|24h` [doc prompt-caching]. [MD
  openai files]: implicit min_tokens 1024; explicit max_breakpoints 4, ttl 30m|12h.
- Effort sets in [MDlive] openai: none/minimal/low/medium/high/xhigh/max appear by model; check per model (see 3.2). [AI openai/resolve-openai-reasoning-effort.ts] exists for mapping.
- Tool search/deferred: namespaced `additional_tools` on GPT-5.5+ (replay without namespace fails on 5.5 and earlier) and `tools.updates` flags per [MD AGENTS.md:100-104]; details [unverified: recall] beyond that.
- Other server tools (web_search, file_search, code_interpreter, computer, mcp, image_generation): the SDK handles code_interpreter and image_generation streams [AI]; full list [unverified: recall].
- Error taxonomy / headers (x-ratelimit-*, insufficient_quota non-retryable): [unverified: recall]. [AI openai-error.ts, openai-stream-error.ts] exist for reading.
- Ollama and llama.cpp/vLLM/LM Studio also serve /v1/responses: stateless only on Ollama (no previous_response_id) [src ollama docs/api/openai-compatibility.mdx:388].

### 2.3 OpenAI Chat Completions and dialects
- Tool-call stream assembly: the generic adapter buffers by `index` until `function.name` is known; Google's compat endpoint sends no index [AI openai-compatible/src/chat/openai-compatible-chat-language-model.ts:616-670,1107].
- Reasoning key: reads `reasoning_content` ?? `reasoning` on response and delta [AI same file :450,:824]; writes `reasoning_content` back [AI convert-to-openai-compatible-chat-messages.ts:310]. [MD] per-model
  `compatibility.reasoning_field` counts: reasoning_content 1407 tables, reasoning_details 102 (OpenRouter), others use default; max_tokens_field override for some hosts (218), e.g. Ollama ignores max_completion_tokens (live probe
  2026-10-24 in MD comment).
- Usage: `prompt_tokens_details.cached_tokens`, `completion_tokens_details.reasoning_tokens` [AI openai-compatible convert-openai-compatible-chat-usage.ts:25-39].
- DeepSeek: see section 1 rule; usage `prompt_cache_hit_tokens`/`prompt_cache_miss_tokens` [AI deepseek/convert-to-deepseek-usage.ts]; docs: thinking toggle `thinking:{type:enabled|disabled}` (OpenAI form) or `reasoning.effort`
  none/low/high/max (Anthropic form), `reasoning_effort` low|high|max, temperature/penalties ignored in thinking, tool_choice none disables tools on v4-pro [doc: api-docs.deepseek.com/guides/thinking_mode]. DeepSeek serves
  chat-completions and messages tables [MD].
- Mistral: tool call ids must match `[a-zA-Z0-9]{9}` [LL litellm/llms/mistral/common_utils.py:16-18]; thinking replayed as content part `{type:'thinking', thinking:[{type:'text',text}]}` [AI
  mistral/convert-to-mistral-chat-messages.ts:150-151].
- Groq: `reasoning_format`, `reasoning_effort`; GPT-OSS cannot disable reasoning, "none" maps to lowest [AI groq-chat-language-model.ts:226-290]. Groq effort enums none|default|low|medium|high [MDlive].
- Cerebras: `reasoning_format` parameter [AI cerebras-provider.ts:68]; models.dev lists only 2 Cerebras models (gpt-oss-120b, qwen-3.8-27b) [MDlive].
- xAI: Responses adapter replays reasoning via item id or `encrypted_content` and warns when neither exists [AI xai/responses/convert-to-xai-responses-input.ts:177-208]. [MD] xai: responses 7, chat-completions 6 tables.
- OpenRouter: `reasoning_details` types reasoning.summary | reasoning.encrypted | reasoning.text [ORP src/schemas/reasoning-details.ts:5-8]; replay-verbatim rule [unverified: recall] (consistent with [MD] marking 102 tables
  reasoning_details). Live model list: pricing keys prompt, completion, input_cache_read, input_cache_write, input_cache_write_1h, web_search, internal_reasoning, image, audio, image_output, audio_output, input_audio_cache,
  request, overrides [ORlive]; supported_parameters include reasoning, include_reasoning, reasoning_effort, verbosity, structured_outputs, tools, parallel_tool_calls, web_search_options, prediction [ORlive]; 358 of 473 models list
  `reasoning`, 411 list `tools`.
- Together/Fireworks/Z.ai/Moonshot/Alibaba/Nvidia/HF/Cloudflare/Vercel: reasoning field = reasoning_content in [MD] (together 17, fireworks 24, zai 21, moonshot 6, alibaba 19, nvidia 10, huggingface 18, cloudflare 2, vercel 5
  model tables); MiniMax tables also carry reasoning_content on chat-completions but MiniMax's default surface in MD is the Anthropic-compatible endpoint (https://api.minimax.io/anthropic/v1, npm @ai-sdk/anthropic).
- Moonshot adapter writes `reasoning_content` on assistant history [AI moonshotai/convert-to-moonshotai-chat-messages.ts:352]. Whether Moonshot/Z.ai/MiniMax require it: not shown by source ([unverified: recall]); [MD
  AGENTS.md:144] states replaying reasoning is the v2 default.

### 2.4 Gemini generateContent and Interactions
- generateContent: functionCall replay without `thoughtSignature` on Gemini 3 models gives HTTP 400; adapter injects `skip_thought_signature_validator` and warns [AI google/convert-to-google-messages.ts:49-54,815-826].
  Signature-bearing parts include text, reasoning and tool parts [AI :480-581].
- Streaming function args: `streamFunctionCallArguments` option and `partialArgs` accumulator [AI google-language-model-options.ts:199-207; google-json-accumulator.ts]. So args can be incremental on newer models.
- Thinking control: `thinking_level` per model: Gemini 4 Neon/Hydrogen low|medium|high|xhigh; 3.8 Flash low|medium|high; 3.6 Flash minimal..high; 3.1 Pro low|medium|high; 3 Pro low|high; 2.5 Pro/Flash low|medium|high [doc:
  aistudio.google.com/docs/thinking]; budget variants (min/max token budgets, e.g. 24576 / 32768) also in [MDlive] google reasoning_options. generateContent field names `thinkingConfig.thinkingLevel` [AI
  google-language-model-options.ts:23].
- Interactions API: stateful with `store:true` + `previous_interaction_id`; stateless needs all `thought` steps replayed exactly; signature only on thought and built-in tool steps; stream: `thought_summary` deltas and a
  `thought_signature` delta before `step.stop` [doc: aistudio.google.com/docs/thinking, /docs/thought-signatures]. [MD]: google api tables: generate-content 30, interactions 37, chat-completions 22.
- Cache: implicit (min_tokens 4096 on some) and explicit [MD google files]. Usage field names, tool/server tools list, structured output field: [unverified: recall].

### 2.5 Bedrock Converse
- Reasoning blocks: `reasoningContent` with `reasoningText{text,signature}`, `redactedReasoning{data}` or `redactedContent`; adapter keeps signature for replay [AI amazon-bedrock-chat-language-model.ts:851-883].
- Stream not SSE: bedrock-runtime uses binary event framing [doc: streaming page note]; adapter decodes exception frames modelStreamErrorException, serviceUnavailableException, throttlingException [AI
  amazon-bedrock-chat-language-model.ts:1099-1114; amazon-bedrock-event-stream-decoder.ts].
- stopReason passthrough kept raw [AI :970,1125]. Auth env: AWS_ACCESS_KEY_ID/SECRET/REGION or AWS_BEARER_TOKEN_BEDROCK [MD].
- [MD] bedrock tables: converse 39, chat-completions 44, responses 19, messages 6; model ids carry region prefixes (`us.`, `eu.`, `jp.`); cache explicit (max 4 breakpoints, min_tokens 512/1024) for Claude, implicit for others.
- Anthropic server tools/tool search on Bedrock only through InvokeModel [doc]. cachePoint JSON shape, usage names, error retry class list: [unverified: recall].
- **Read 2026-11-25** [doc: docs.aws.amazon.com/bedrock/latest, smithy.io/2.0; each page is named where the code rests on it: `model/src/codec/converse.rs`, `eventstream.rs`, `profile.rs`, `error.rs`], which settles:
  - Key: `Authorization: Bearer`, env name `AWS_BEARER_TOKEN_BEDROCK`; for Bedrock and Bedrock Runtime actions only; short-term ones last at most 12 h and hold in one region, long-term ones are for trying things out, says AWS (userguide/api-keys-use,
    api-keys-reference). Region lists differ from page to page (api-keys-supported, general/latest/gr/bedrock), so none is kept here.
  - Frames: prelude, two CRC32s, ten header types; `:message-type` is `event`, `exception` (`:exception-type`) or `error` (`:error-code`, `:error-message`), also after HTTP 200 (smithy.io amazon-eventstream).
  - Order: messageStart, per block start (tool use only), deltas, stop; messageStop; metadata with usage. `cachePoint {type: default, ttl?}`; Claude: tools, system, messages and 5m|1h; Nova: system and messages, 5m, no `ttl`; GPT: none in Converse (userguide/prompt-caching).
    With a cache `inputTokens` is what was neither read nor written; reasoning tokens are not told.
  - Claude's effort: `additionalModelRequestFields {thinking: {type: adaptive}, output_config: {effort}}` (userguide/claude-messages-adaptive-thinking#converse). The reference also has a top-level `outputConfig.effort`; which models take it is not said, so it is not used. Nova 2
    has `reasoningConfig` (nova2-userguide/extended-thinking); not built.
  - Reasoning: signed and redacted blocks go back unchanged, empty texts included; unsigned ones do not (DeepSeek's example takes them out, Nova 2's are the word `[REDACTED]`). Preserved thinking binds blocks to what comes before them (claude-messages-thinking-block-binding).
  - Ids of uses `[a-zA-Z0-9_.:-]{1,64}`, names of tools `[a-zA-Z0-9_-]{1,64}`, a description is not empty. `status: error` of a result is "only supported by Amazon Nova and Anthropic Claude 3 and 4", though the page on tool use sends it to Cohere.
  - Some models refuse tools in a stream: Llama 3.1 to 4, Mistral Large 1 and 2, Pixtral, Palmyra (userguide/tool-use-client-side). They are told apart by the error only.
- **Not found on any page**, and so remembered or assumed: that turns must alternate (they are joined), the words for a prompt that is too long ("Input is too long"), that blank texts are refused, how a call without arguments is streamed, whether a signature comes in pieces
  (they are joined), whether `toolConfig` may be left out when the history has uses of tools (Axiom's agents always have tools). No key has been used.

### 2.6 Vertex and Azure
- Vertex: [MD] google-vertex tables generate-content 17, messages 18, responses 8, chat-completions 23, interactions 12; separate provider `google-vertex-anthropic` (18 Claude models, npm @ai-sdk/google-vertex/anthropic); env
  GOOGLE_VERTEX_PROJECT/LOCATION/ADC. Endpoint paths (rawPredict, anthropic_version vertex body field): [unverified: recall].
- Azure OpenAI: [MD] azure 101 files: responses 29, chat-completions 62, messages 16 (Claude on Foundry); env AZURE_RESOURCE_NAME, AZURE_API_KEY; implicit cache min 1024, ttl 24h. `model` = deployment name, content-filter
  semantics, header names: [unverified: recall].

## 3. PROVIDER CAPABILITY MATRIX
Source for every cell unless noted: [MD] provider files / [MDlive] (api tables, cache, effort, base url) and [ORlive] for OpenRouter. "?" = not shown by any source read ([unverified: recall] omitted on purpose).
### 3.1 Surfaces and cache (counts = model tables in [MD] providers/<p>/models)
| Provider | API tables (count) | Base URL / npm [MD] | Cache kinds [MD] |
|---|---|---|---|
| anthropic | messages 18, chat-completions 18 | @ai-sdk/anthropic | explicit: ttl 5m/1h, 4 bp, min 512/1024/4096 |
| openai | responses 28, chat-completions 32, realtime 12 | @ai-sdk/openai | implicit min 1024 (+ttl 24h); GPT-5.6: explicit ttl 30m/12h, 4 bp |
| google | generate-content 30, interactions 37, chat-completions 22 | @ai-sdk/google | implicit(min 4096 some)+explicit; 22 models none |
| deepseek | chat-completions 4, messages 4 | https://api.deepseek.com | implicit |
| openrouter | chat-completions 366, responses 356, messages 356 | https://openrouter.ai/api/v1 | mixed: implicit, explicit max 1 bp ttl 5m, explicit ttl 30m |
| xai | responses 7, chat-completions 6 | @ai-sdk/xai | implicit |
| mistral | chat-completions 2 (+fim, ocr, realtime) | @ai-sdk/mistral | implicit min 64 |
| togetherai | chat-completions 17 | @ai-sdk/togetherai | implicit |
| groq | chat-completions 9, responses 9 | @ai-sdk/groq | implicit ttl 2h on 3 models |
| cerebras | (default chat) | @ai-sdk/cerebras | implicit min 128 ttl 5m |
| fireworks-ai | chat-completions 27, responses 29, messages 29 | https://api.fireworks.ai/inference/v1/ | implicit |
| azure | responses 29, chat-completions 62, messages 16 | @ai-sdk/azure | implicit min 1024 (ttl 24h); explicit ttl 30m on some |
| amazon-bedrock | converse 39, chat-completions 44, responses 19, messages 6 | @ai-sdk/amazon-bedrock | explicit 4 bp (Claude); implicit others |
| google-vertex | generate-content 17, messages 18, responses 8, chat-completions 23 | @ai-sdk/google-vertex | implicit (min 4096) / none |
| huggingface | chat-completions 92, responses 92 | https://router.huggingface.co/v1 | not stated |
| zai | chat-completions 21, messages 21, responses 21 | https://api.z.ai/api/paas/v4 | implicit |
| moonshotai | chat-completions 6, responses 6, messages 6 | https://api.moonshot.ai/v1 | implicit (ttl 5m/1h on 3) |
| minimax | messages 8, chat-completions 8, responses 8 | https://api.minimax.io/anthropic/v1 | explicit + implicit min 512 |
| alibaba (Qwen) | chat-completions 59, messages 36, responses 29 | https://dashscope-intl.aliyuncs.com/compatible-mode/v1 | explicit (system, messages; 4 bp) on 16; implicit; none on 10 |
| nvidia | chat-completions 10 | https://integrate.api.nvidia.com/v1 | not stated |
| vercel gateway | chat-completions 261, responses 261, messages 261 | @ai-sdk/gateway | implicit 179, explicit+implicit 32 |
| cloudflare workers-ai | chat-completions 6, workers-ai-run 13, responses 2 | https://api.cloudflare.com/client/v4/accounts/${ID}/ai/v1 | none 18, implicit 9 |
| ollama-cloud / lmstudio | chat/responses/messages 28 / (default) | https://ollama.com/v1 / http://127.0.0.1:1234/v1 | implicit / not stated |
### 3.2 Reasoning controls published in [MDlive] (union over each provider's reasoning models; `effort` value sets)
| Provider | Effort value sets seen | Budget / toggle | Replay field [MD interleaved] |
|---|---|---|---|
| anthropic | low-medium-high; +max; +xhigh,max | budget_tokens min 1024 (older); toggle | n/a (native blocks) |
| openai | high; low-medium-high; minimal..high; medium-high-xhigh; none..max | none | n/a |
| google | low-high; low-medium-high; +xhigh | budget 0..24576 / 128..32768 | n/a |
| deepseek | low-high-max | toggle | reasoning_content |
| xai | low-medium-high-xhigh; none..xhigh | none | none set |
| mistral | low-high-max; none-high; none-high-max; low-medium-high | none | none set |
| groq | low-medium-high; none-default-low..high | none | none set |
| cerebras | low-medium-high; none..high | none | ? |
| togetherai / fireworks / huggingface | many sets incl. minimal, max | none | reasoning_content |
| zai / moonshotai | high-max; low-high-max | toggle | reasoning_content |
| minimax | low..max | toggle | none set |
| alibaba | high-max; low-high-max | budget up to 262144 (e.g. 81920) | reasoning_content on 9 |
| nvidia | none/low/medium/high/max variants | budget -1..32768 | reasoning_content |
| openrouter | high; low-high; high-xhigh-max | budget 128..32768 | reasoning_details (9), reasoning_content (10), else none |
| lmstudio local | low-medium-high | none | ? |
Rule from [MD AGENTS.md:287-317]: a host's effort list is host-specific; never assume a lab's list applies to a relay.
### 3.3 Capability fields available per model in each metadata source
| Source | Fields relevant to the matrix |
|---|---|
| [MDlive] api.json | reasoning, reasoning_options[effort/budget/toggle], tool_call, structured_output, temperature(bool), attachment, modalities, limit{context,output,input}, cost{input,output,cache_read,cache_write,tiers,context_over_200k,reasoning}, interleaved{field}, status, canonical_model_id |
| [MD] v2 TOML | capabilities.tools{supported,strict,choice,search,freeform,updates}, system_updates, effort_updates, cache{implicit,explicit{ttl,max_breakpoints,targets,min_tokens}}, service_tier, [api.<protocol>] tables with compatibility{reasoning_field,max_tokens_field}, cost.multipliers, web_search cost |
| [ORlive] | per model: context_length, max_completion_tokens, pricing (14 keys), supported_parameters, input/output modalities, endpoints[], default_parameters, knowledge_cutoff, expiration_date, hugging_face_id, benchmarks |
| [LLjson] | supports_function_calling, parallel_function_calling, vision, pdf_input, reasoning, prompt_caching, response_schema, tool_choice, native_structured_output, adaptive_thinking, xhigh/max/minimal/none_reasoning_effort, mid_conversation_system, output_config, assistant_prefill, sampling_params, web_search, computer_use; cost incl. _priority/_flex/_batches/_above_200k/_above_272k, cache_creation_above_1hr, prompt_cache_min_tokens |

## 4. Cross-provider concerns
- Context overflow: Bedrock reports `model_context_window_exceeded` as stopReason on success ([unverified: recall]); others errors; normalise to one class [design].
- Tool-id rules: Mistral 9 alnum [LL]; others [unverified: recall]. Keep canonical id + per-provider remap persisted per session [design].
- Anthropic mid-session tool/system changes: use API-supported additions to keep prefix (flags in [MD]); changing earlier prefix invalidates thinking and cache [doc preserved-thinking].
- Switching Claude models inside one history is supported by the API (drops unreadable blocks, no error, not billed) [doc preserved-thinking]; do not keep per-model histories.
- Another provider's turn goes at the end of a Claude conversation as text/tool_use only; earlier Claude thinking stays valid [doc preserved-thinking].
- Sampling params: Claude Sonnet 5 `temperature:false` in [MDlive]; OpenAI GPT-5 base `temperature=false, top_p=false` in the model template example [MD README]; rule = per-model capability flags drive request building.
- Token counting endpoints: Anthropic `/v1/messages/count_tokens` exists for Claude and llama.cpp mirrors it [src llama.cpp tools/server/README.md:1891]; OpenAI `/v1/responses/input_tokens` exists in llama.cpp's mirror
  (README:1812) [others: unverified: recall].
- Cancellation, retry taxonomy for the long tail, rate-limit header names for OpenAI/Groq/Together/etc.: [unverified: recall]; build as data table validated by live probes.

## 5. Smallest codec set and quirk profiles (opinion, supported by [GN]/[RG]/[AI] shapes)
- Evidence for five codecs: rust-genai ships adapters anthropic, openai, openai_resp, gemini, vertex, bedrock, cohere, ollama (+ thin zai, fireworks, omlx, github_copilot) [GN src/adapter/adapters]; AI SDK has separate packages
  for anthropic, openai (chat + responses), google, amazon-bedrock, and a shared openai-compatible base used by deepseek, moonshotai, zai, minimax, fireworks, togetherai, cerebras, groq, alibaba etc. [AI packages]. Models.dev's
  own protocol list is the same families: messages, responses, chat-completions, generate-content, converse (+interactions, cohere-chat) [MD].
- Codecs: (1) anthropic_messages [Anthropic, Vertex Claude, Foundry Claude, Bedrock InvokeModel, MiniMax default, and the `messages` tables of DeepSeek/Fireworks/Moonshot/Z.ai/Alibaba/OpenRouter/Vercel/Ollama]; (2)
  openai_responses [OpenAI, Azure, xAI preferred, Groq, OpenRouter, HF, local servers (stateless)]; (3) openai_chat [long tail, all local]; (4) gemini [generateContent; Interactions as a later 6th codec if stateful features
  matter]; (5) bedrock_converse. Adopting [MD]'s per-model `[api.*]` order and `recommended_api` lets the codec choice be data, not code.
- Quirk hooks (data per provider x model): reasoning request/response field names and replay policy; tool-call assembly (index/id/whole); tool-id remap; JSON-schema downgrade; accepted sampling params + max_tokens field
  (`max_tokens_field` in [MD]); usage convention; cache strategy (explicit/implicit, ttl, max breakpoints, min tokens from [MD]); error table; inline `<think>` splitter; message-shape fixups; mid-conversation update support flags
  (`system_updates`, `effort_updates`, `tools.updates` from [MD]).
- Seed profiles from [MD] + [LLjson] rather than hand-writing; override with live probes.

## 6. Internal event / content-block model (design, grounded in adapter field lists above)
- Keep per block: native JSON fragment + codec/model-family tag + validity scope (account/model/prefix hash for Anthropic thinking; item id/encrypted for OpenAI; thoughtSignature for Gemini; reasoningText.signature for Bedrock;
  reasoning_details for OpenRouter).
- Block kinds required by sources: text(+citations, phase commentary|final_answer), reasoning{plain|summary, signature, encrypted, redacted, id}, tool_call{canonical id, provider ids (call_id + item id; tool_use id), args raw
  string, caller, thought_signature, origin client|server|mcp}, tool_result, server_tool (tool_search results with tool_reference blocks), compaction{encrypted payload, summary}, media, cache_marker{ttl,mode}, unknown.
- Events: block_start/delta/stop incl. signature_delta and thought_summary deltas, tool_args_delta, partial-args for Gemini, usage_update(cumulative), error mid-stream, keepalive/ping and provider comment lines (OpenRouter
  keep-alives [unverified: recall]).
- Usage normal form: uncached input, cache read, cache write (+ttl split: Anthropic 5m/1h, OpenAI ttl_30m/ttl_12h), output, reasoning, cost estimate, raw [doc: OpenAI prompt-caching; MDlive cost.cache_write per ttl].
- History must be append-only and prefix-stable (Anthropic prefix check; OpenAI "keep the prefix unchanged"; local prefix caches) [doc preserved-thinking, conversation-state].
- Model switch: replay planner with lossy-conversion report; Claude-to-Claude may pass history untouched (API drops), cross-vendor drops reasoning, remaps ids, injects Gemini sentinel [doc + AI].

## 7. Pricing and model metadata sources (Part 3)
| Source | Licence | Shape / size | Notes |
|---|---|---|---|
| models.dev | MIT [MD LICENSE: (c) 2025 models.dev] | api.json 6.9 MB, 326 providers, 10181 models (fetched 2026-11-24T13:03Z); also `models.json` (lab-level), `catalog.json`, `model-schema.json`, experimental v2 `/experimental/v2.0/api.json` with `type` filters [MD README] | Prices are USD per 1M tokens (e.g. Sonnet 5 input 2 / output 10 / cache_read 0.2 / cache_write 2.5 [MDlive]); `cost.tiers`, `context_over_200k` (784 models); `reasoning_options`; each provider file has sourced comments; community-maintained, sync tooling in repo |
| OpenRouter /api/v1/models | terms not reviewed ([unverified]) | 918 KB, 473 models; pricing strings are USD per token; includes `:batch` variants as separate ids [ORlive] | Best live capability probe for OpenRouter only; `endpoints` shows which surfaces a model serves; per-model `links.details` -> endpoints list (not fetched) |
| LiteLLM json | MIT for repo, `litellm/enterprise/` carved out; the json sits at repo root and the enterprise dir was absent in the shallow clone [LL LICENSE] | 5.2 MB, 7158 keys; `sample_spec` documents the schema; modes chat 4512, image_generation 1042, video 524, responses 229, embedding 171 [LLjson] | Region/profile-specific ids for Bedrock/Vertex/Azure; `supports_*` flags; tiers `_above_200k`/`_above_272k`, `_priority`, `_flex`, `_batches`; skip `sample_spec` |
Offline-first plan (design): bundle a build-time merged snapshot with generated_at + per-source commit/ETag + SPDX notices; daily conditional GET refresh with schema validation, atomic swap, last-good retention; field precedence
user override > live provider discovery/probe > models.dev > LiteLLM > OpenRouter > snapshot, provenance stored per field; never block requests; unknown price shows "no price"; store `price_version` on usage records; alias table
(dated ids, region prefixes, `@date`, `:free/:batch` suffixes) to a canonical family key (models.dev `canonical_model_id` helps [MD README:238]); weekly CI drift diff.

## 8. Unverified (explicit)
- OpenAI error taxonomy (insufficient_quota non-retryable), OpenAI/Anthropic/Groq/Together/Fireworks/Cerebras rate-limit header names, Anthropic usage field names beyond the adapter file name, Gemini usageMetadata field names,
  Bedrock cachePoint and usage names, Vertex rawPredict specifics, Azure deployment/content-filter behaviour, OpenRouter reasoning_details replay rule and keep-alive comment lines and ToS, xAI/Together/Fireworks/Cerebras request
  params beyond [AI]/[MD], Mistral post-tool message ordering, MiniMax/Moonshot/Z.ai required-replay flags, Qwen enable_thinking/stream-only rule, NIM chat_template_kwargs, Vercel/Cloudflare specifics, HF `:fastest` suffix
  routing, exact OpenAI tool-search shapes.
- anthropic-version value policy (2023-06-01 still accepted?).
- Whether each [MD] "implicit" cache entry reports cached tokens in usage on every host.

## 9. Closure of the previous "needs checking first" list
1. Effort/thinking enums newest models: CLOSED from [MDlive] + [doc] (3.2, 2.1, 2.4); Anthropic adaptive/effort/between_tools confirmed [doc errors].
2. OpenAI tool-search/deferred shapes: PARTIAL. Namespaced additional_tools and flags in [MD AGENTS.md]; field shapes open (section 8). Anthropic tool search: CLOSED [doc].
3. Anthropic compaction block/header: CLOSED (`compact-2026-09-04`, `compaction` param, block `compaction`) [doc, AI]. OpenAI compact endpoint: CLOSED names (`/responses/compact`, `context_management.compact_threshold`) [doc];
   body shapes not read.
4. Gemini 3 streamed args: CLOSED (`streamFunctionCallArguments`, `partialArgs`) [AI]. Dummy signature: CLOSED (`skip_thought_signature_validator`) [AI]. Non-first parallel-call signature rule: still open.
5. DeepSeek reasoning replay: CLOSED and my draft was wrong (section 1).
6. Groq gpt-oss pass-back: OPEN (only effort handling seen in [AI]). Moonshot/MiniMax/Z.ai preserved-thinking flags: OPEN; [MD] `interleaved.field=reasoning_content` for zai(18)/moonshot(6) and null for minimax (8) is the only
   evidence.
7. Mistral ordering rule: OPEN. Mistral 9-char ids: CLOSED [LL].
8. Fireworks cache headers: OPEN (MD says implicit only). Mistral/Together cached-token fields: OPEN. xAI Anthropic-compat: not listed in [MD] xai tables (responses/chat only), so treat as absent. OpenRouter Responses/Messages:
   CLOSED, they exist [ORlive]. Azure Foundry Claude path: OPEN (MD azure has 16 `messages` tables).
9. models.dev licence: CLOSED MIT. LiteLLM json licence boundary: CLOSED as MIT-except-enterprise (dir not present in clone). OpenRouter ToS: OPEN.
10. Open-source adapter behaviour: CLOSED for [AI] (anthropic, openai, google, openai-compatible, deepseek, mistral, groq, xai, bedrock, moonshotai, cerebras), [LL] (mistral), [ORP] (reasoning details), [GN]/[RG] (adapter
    inventory only; code paths not audited).

