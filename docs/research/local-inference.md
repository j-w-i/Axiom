# Local inference research

Date: 2026-11-24 (fetches 13:03 UTC). Revision 2: claims are tagged; recall that nothing supports was deleted or kept only where a design decision needs it.

## 0. Evidence legend
**[src: X@sha]** file read in a read-only shallow clone under `scratchpad/research/prov/`: llama.cpp@9578bd3 (tools/server/README.md, source tree), ollama@924cc28 (docs/*.mdx, docs/openapi.yaml), mlx-lm@1bf8da0 (mlx_lm/server.py, SERVER.md, tool_parsers/),
vllm@b09bc99 (vllm/entrypoints, vllm/reasoning, vllm/tool_parsers, config/cache.py), rust-genai@2a8a5b3, models.dev@6e1fdc3. **[doc: url]** page read via WebFetch 2026-11-24. **[live: lms]** local `lms` CLI (help, `server status`, `ps`, `ls`, `server start --help`; no models loaded,
no server started, no curl to localhost). **[MDlive]** models.dev/api.json 2026-11-24T13:03Z. **[unverified: recall]** training memory only.
Not cloned this pass (so only recall): SGLang, LocalAI, Jan, KoboldCpp, mlx-vlm, LM Studio SDKs. Keep them out of decisions until read.

## 1. This machine [live: lms]
- LM Studio server: "running on port 1234"; `lms ps`: no models loaded. `lms ls`: 4 models, 3.31 GB: qwen/qwen3-4b-2507 (4B, qwen3, 2.28 GB), qwen2.5-0.5b-instruct-mlx (0.5B, 294 MB), qwen3.5-0.8b-mlx (0.8B, qwen3_5, 652 MB), text-embedding-nomic-embed-text-v1.5 (84 MB).
- `lms` commands: chat, get, load, unload, ls, ps, import, server, log, link (LM Link remote instances), runtime, clone/push/dev/login (hub). `lms server start`: `-p/--port` (reuses last port), `--bind` (127.0.0.1 default; 0.0.0.0 exposes LAN; env LMS_SERVER_HOST), `--cors`.
- Implication: probe, never assume a model is loaded; a remote LM Link model may look local (behaviour of link not read).

## 2. Comparison table (all cells tagged by the section that proves them)
| Server | Default port | Identify / health | Model list | Load / unload | Context control | Speed stats | Cache signal | Reasoning field | Tools |
|---|---|---|---|---|---|---|---|---|---|
| Ollama | 11434 (docs examples) [src ollama docs] | `/api/version` ?; `/api/tags` | `/api/tags`, `/api/show`, `/api/ps` | implicit; `keep_alive` 0/-1 [src faq.mdx:322-343] | `num_ctx`, `OLLAMA_CONTEXT_LENGTH` [src] | `load_duration`, `prompt_eval_*`, `eval_*` [src openapi.yaml] | `prompt_eval_cached_count` [src] | `message.thinking` [src] | native `message.tool_calls` [src] |
| llama.cpp | 8080 [src README:337-346] | `/props`, `/health` | `/v1/models`; router `/models` | router `POST /models/load|unload` {model} [src README:2394-2430] | `-c`, kv-unified, `--kv-unified-per-slot` | `timings` [src README:1544-1566] | `timings.cache_n` [src] | `reasoning_format`, `reasoning_content` ? | `--jinja` (default on) [src] |
| LM Studio | 1234 [live: lms] | `/api/v1/models` | `/api/v1/models` [doc] | `POST /api/v1/models/load|unload` [doc] | `context_length` at load/chat [doc] | `stats{tokens_per_second,time_to_first_token_seconds,...}` [doc] | not documented | `reasoning` output item [doc] | none for custom tools on /api/v1/chat [doc] |
| mlx-lm | 8080 on 127.0.0.1 [src server.py:1924-1932] | `/health` [src :1782] | `/v1/models` [src :1780] | model arg per request / start | `--max-tokens`; cache size flags | not in usage ? | `usage.prompt_tokens_details.cached_tokens` [src :1506-1513] | `reasoning` [src :1528] | `tool_parsers/` [src] |
| vLLM | 8000 ? [unverified: recall] | `/health`, `/load` [src serve/instrumentator] | `/v1/models` [src] | one model per process | launch-time | `/metrics` [src] | prefix caching default True [src config/cache.py:267] | `reasoning` (renames `reasoning_content`) [src] | `vllm/tool_parsers` [src] |
| SGLang / LocalAI / Jan / KoboldCpp | 30000 / 8080 / 1337 / 5001 [unverified: recall] | not read | not read | not read | not read | not read | not read | not read | not read |

## 3. Per-server verified detail
### Ollama [src ollama@924cc28]
- Context: docs/context-length.mdx: defaults by VRAM: <24 GiB 4k, 24-48 GiB 32k, >=48 GiB 256k; agent tasks should be at least 64000; cloud models default to max. docs/faq.mdx:44-52 still says 4096 by default. The two pages disagree: read `/api/ps` `context_length` instead of trusting either.
  Override with `OLLAMA_CONTEXT_LENGTH` or `options.num_ctx` per request (faq.mdx:52-71).
- `/api/chat` and `/api/generate` stream NDJSON (`application/x-ndjson`), `stream:false` for single JSON [docs/api/streaming.mdx:5-19]. Final stats: `load_duration`, `prompt_eval_count`, `prompt_eval_cached_count`, `prompt_eval_duration`, `eval_count`, `eval_duration` [docs/openapi.yaml:140-202,347-362].
- `/api/ps` fields: `expires_at`, `size_vram`, `context_length` [openapi.yaml:725-731]. `/api/show` returns `capabilities` (e.g. thinking) and `model_info` (e.g. `gemma4.context_length: 131072`) and thinking controls metadata [openapi.yaml:406-424,1793-1828].
- Thinking: request `think` (bool or named level; supported values discovered via `/api/show`), response `message.thinking` and chunked `thinking` [openapi.yaml:98-128,175,292-327,389-406]. OpenAI-compat `reasoning_effort`/`reasoning.effort` resolve against that metadata; unsupported names fall to the model default; GPT-OSS cannot disable (maps none to low)
  [docs/api/openai-compatibility.mdx:245-253].
- Keep-alive: `keep_alive` on generate/chat overrides `OLLAMA_KEEP_ALIVE`; 0 unloads, -1 keeps [faq.mdx:322-343]. Env: `OLLAMA_MAX_LOADED_MODELS` default 3 per GPU (3 CPU), `OLLAMA_NUM_PARALLEL`, `OLLAMA_KV_CACHE_TYPE` default f16 [faq.mdx:365-385]; flash attention automatic when backend supports it [faq.mdx:377].
- Compat surfaces: `/v1/responses` stateless only (no previous_response_id or conversation) [openai-compatibility.mdx:382-388]; Anthropic Messages subset: messages, streaming, function calling; tool_choice controls, deferred tools, hosted web search not fully supported; no cache_control [docs/api/anthropic-compatibility.mdx:5-36].
  `max_tokens` is honoured, `max_completion_tokens` ignored on the compat chat API (live probe 2026-10-24) [src models.dev@6e1fdc3 provider comments].
- Tool calling: response carries `message.tool_calls` [docs/capabilities/tool-calling.mdx]; whether arguments stream partially: not shown in the pages read ([unverified: recall]).
- Memory: `size_vram` per model only; no system memory endpoint seen.

### llama.cpp `llama-server` [src llama.cpp@9578bd3 tools/server/README.md]
- Default listen 8080 in docker examples; `--host`, `--api-key`, `--api-key-file` (README:194,218-219); `--metrics` off by default (:228); `/slots` enabled by default, disable with `--no-slots` (:230,1097); `POST /props` needs `--props` (:229,1049).
- Slots/cache: `-np/--parallel` default -1 auto; `--kv-unified` default enabled when slots are auto; `--kv-unified-per-slot N` sets per-slot limit; `-cram/--cache-ram` default 8192 MiB (0 disables, -1 unlimited); `--cache-idle-slots` default on; `--cache-reuse N` KV-shift reuse default 0;
  `-sps/--slot-prompt-similarity` default 0.10 (README:166-178,227,250). Request `cache_prompt` default true, with a note that results may be non-deterministic across batch sizes (:603). `n_cache_reuse` per request (:548).
- Timings object in OpenAI-style responses: `cache_n`, `prompt_n`, `prompt_ms`, `prompt_per_second`, `predicted_n`, `predicted_ms`, `predicted_per_second`; "also allows calculating current context usage" (:1544-1566). `timings_per_token` option for streams (:609).
- `/props`: `default_generation_settings`, `total_slots`, `model_path`, `chat_template`, `chat_template_caps` (incl. `reasoning_effort_levels`, `reasoning_effort_rejected`), `modalities`, `is_sleeping` ("sleeping on idle") (:1036-1046). `/slots` per-slot state, `/slots/{id}?action=save|restore|erase`.
- Router mode (multi-model): `--models-dir`, `--models-preset` (INI), `--models-max` default 4 (LRU), `--models-memory-margin`; `GET /models` with `status.value` (e.g. "loaded") and `architecture` object; `POST /models/load` and `/models/unload` with `{model}` (README:234-236,2220-2430). `/props?model=` selects per model.
- Chat: `--jinja` default enabled (:239); function calling needs a template that supports tools, may need `--chat-template-file` (:1538); `parallel_tool_calls` honoured only if template supports (:1476); `reasoning_effort: none` disables thinking else passed to the template (:1470);
  `reasoning_format` none keeps raw text (:1472); `--reasoning-budget` -1 unrestricted, 0 immediate end, N token budget (:243).
- Also serves OpenAI Responses (`/v1/responses`, `/v1/responses/input_tokens`) and Anthropic Messages (`/v1/messages`, `/v1/messages/count_tokens`, tools need jinja) (:1703-1900); `/v1/models`; `/tokenize`; reranking, embeddings, TTS endpoints exist.
- Does not report memory in the API read; `/metrics` content not read this pass.

### LM Studio [doc: lmstudio.ai/docs/developer/rest* 2026-11-24]
- v1 REST: `/api/v1/chat`, `/models`, `/models/load`, `/models/unload`, `/models/download`, `/models/download/status`; custom tools are not supported on `/api/v1/chat`; MCP via `integrations` (plugin ids or ephemeral servers with `allowed_tools`).
- `GET /api/v1/models`: `models[]` with `type` (llm|embedding|decision), `publisher`, `key`, `display_name`, `architecture`, `quantization{name,bits_per_weight}`, `size_bytes`, `params_string`, `loaded_instances[{id,config{context_length,eval_batch_size,parallel,flash_attention,num_experts,offload_kv_cache_to_gpu}}]`,
  `max_context_length`, `format` (gguf|mlx), `capabilities{vision,trained_for_tool_use,reasoning{allowed_options,default}}`, `variants`, `selected_variant`.
- `POST /api/v1/models/load`: `model`, `context_length`, `flash_attention`, `eval_batch_size`, `num_experts`, `offload_kv_cache_to_gpu` (llama.cpp-only options), `echo_load_config`; response `type`, `instance_id`, `load_time_seconds`, `status`, optional `load_config`.
- `POST /api/v1/chat`: `model`, `input` (text or items with image `data_url`), `system_prompt`, `integrations`, sampling (`temperature`, `top_p`, `top_k`, `min_p`, `repeat_penalty`, `max_output_tokens`, `context_length`), `reasoning` off|low|medium|high|on, `stream`, `store` (default true) with `response_id`, `previous_response_id`.
  Output item types `message`, `reasoning`, `tool_call`, `invalid_tool_call`. Stats: `input_tokens`, `total_output_tokens`, `reasoning_output_tokens`, `tokens_per_second`, `time_to_first_token_seconds`, `model_load_time_seconds`. No dedicated prefill-speed field.
- v0 REST (older): `/api/v0/models` fields id, type, publisher, arch, compatibility_type, quantization, state, max_context_length; chat completions return `stats{tokens_per_second,time_to_first_token,generation_time,stop_reason}`, `model_info`, `runtime` [doc: .../rest/endpoints].
- OpenAI-compatible endpoints (`/v1/chat/completions`, `/v1/responses`) and cache behaviour, JIT-load and idle TTL semantics: not read this pass ([unverified: recall]). Unload request fields not read.
- Models.dev lists LM Studio at http://127.0.0.1:1234/v1 with `openai/gpt-oss-20b`, `qwen/qwen3-coder-30b`, `qwen/qwen3-30b-a3b-2507` and effort low|medium|high [MDlive].

### mlx-lm [src mlx-lm@1bf8da0]
- `mlx_lm.server`: default host 127.0.0.1, port 8080; POST `/v1/completions`, `/v1/chat/completions` (server.py:1255-1256); GET `/v1/models` (:1780), `/health` (:1782).
- Flags seen: `--model`, `--adapter-path`, `--host`, `--port`, `--allowed-origins`, `--draft-model`, `--num-draft-tokens`, `--trust-remote-code`, `--chat-template`, `--chat-template-args`, `--temp/--top-p/--top-k/--min-p/--max-tokens`, `--decode-concurrency`, `--prompt-concurrency`,
  `--prefill-step-size`, `--prompt-cache-size`, `--prompt-cache-bytes`, `--kv-bits`, `--kv-group-size`, `--quantized-kv-start`, `--pipeline` (server.py:1914-2064).
- SERVER.md:50: a quantized KV cache does not support batching (requests are processed one at a time in that mode).
- Response: `usage.prompt_tokens_details.cached_tokens` present (server.py:1506-1513); reasoning returned as `reasoning` field when parsed (:1527-1528); a reasoning/normal state machine splits streams (:643-713).
- Tool parsers in `mlx_lm/tool_parsers/`: function_gemma, gemma4, glm47, json_tools, kimi_k2, kimi_k3, laguna, longcat, minicpm5, minimax_m2, mistral, muse_spark, pythonic, qwen3_coder. Model quality per parser not tested.
- Memory: not reported by server. Other MLX servers (mlx-vlm, vllm-mlx, omlx): rust-genai has an `omlx` adapter that allows no API key and sets a reasoning block, with a comment "not 100% sure this is honored" [src rust-genai adapters/omlx/adapter_impl.rs:56-66]; nothing else read.

### vLLM [src vllm@b09bc99]
- Endpoints in tree: `/v1/models`, `/v1/responses`, `/v1/messages` (Anthropic), `/health`, `/load`, `/metrics` (Prometheus mount), `/tokenize` (vllm/entrypoints/...). Port default not re-verified.
- `enable_prefix_caching: bool = True` (config/cache.py:267). Chat protocol renames deprecated `reasoning_content` to `reasoning` (openai/chat_completion/protocol.py:594-610): clients must read both.
- Large per-model parser registries: `vllm/reasoning/*` (deepseek_r1, deepseek_v4, gptoss, glm47, kimi_k2/k3, minimax_m2/m3, mistral, qwen3 ...) and `vllm/tool_parsers/*` (hermes, llama_json, mistral, deepseekv3/v31/v32/v4, glm47, kimi_k2/k3, minimax_m2/m3, gptoss, granite, phi4mini ...). Wrong or missing parser flag symptoms:
  [unverified: recall] but the registry confirms parsers are per model family and server-side.

## 4. Decisions and guidance for Axiom (design, not claims)
- **Discovery**: probe loopback only, in parallel with short timeouts: Ollama (`/api/tags`), LM Studio (`/api/v1/models`), llama.cpp (`/props`, then `/models` for router), mlx-lm (`/health` + `/v1/models`), vLLM (`/health` + `/v1/models`). Identify by response shape, not port (8080 is both llama.cpp and mlx-lm defaults [src]).
  Use `lms server status` for LM Studio when the CLI exists. Never start servers or load models without consent.
- **Context hazard handling**: read the configured context, not the max: Ollama `/api/ps`, LM Studio `loaded_instances[].config.context_length`, llama.cpp `/props` + `/slots`, vLLM launch args. Warn or compact when prompt + tools exceed it. Reload with larger context only where an API exists (Ollama option; LM Studio `/models/load`; llama.cpp router `/models/load`).
- **Speed stats**: Ollama (ns durations), llama.cpp (`timings`), LM Studio v1 (`stats`), else client-side stream timing; vLLM via `/metrics`.
- **Cache signal**: Ollama `prompt_eval_cached_count`, llama.cpp `timings.cache_n`, mlx-lm `cached_tokens`; keep prefixes byte-stable. llama.cpp slot choice uses prompt similarity (default 0.10), so parallel agents need separate slots or accept re-prefill.
- **Reasoning normalisation**: accept `message.thinking` (Ollama), `reasoning_content` and `reasoning` (llama.cpp/vLLM/mlx-lm), LM Studio `reasoning` items, plus an inline `<think>` splitter; no signatures locally, so replay policy is per-template data.
- **Tool-call reliability**: server-side parsers are per model family everywhere (llama.cpp jinja template caps, vLLM/mlx-lm parser registries). Run a startup tool round-trip probe; fall back to prompted-tool mode when it fails. Real quality per model: not measured.
- **LM Studio specifics**: for Axiom's own tool loop use its OpenAI-compatible surface (custom tools unsupported on `/api/v1/chat`); use v1 for list/load/unload and capability metadata (`trained_for_tool_use`, `reasoning.allowed_options`).
- **Memory**: no cross-server memory API in the sources read; read OS memory and per-server sizes (`size_vram`, `size_bytes`) [design].

## 5. Unverified
- SGLang, LocalAI, Jan, KoboldCpp, mlx-vlm: everything (ports 30000/8080/1337/5001 are recall).
- Default ports for Ollama, vLLM as shipped (only doc/example evidence for Ollama; vLLM not checked).
- LM Studio: OpenAI-compat behaviour, JIT load/TTL semantics, unload payload, cache behaviour, LM Link semantics.
- Ollama: partial streaming of tool-call arguments; exact `/api/version` output.
- llama.cpp: `/metrics` metric names; behaviour of `--kv-unified` with explicit `-np`; per-template tool parser coverage.
- vLLM: default port, tool/reasoning flag semantics and memory reporting.
- All real tool-call quality and prefill/decode speeds (nothing was loaded).

## 6. Closure of the earlier checklist items (local)
- Ollama default ctx: CLOSED (VRAM tiers vs faq 4096 conflict, section 3). llama.cpp router mode, `--cache-ram`: CLOSED (present, defaults above). mlx-lm cache flags and tool parsers: CLOSED (flags and parser files listed). vLLM Anthropic endpoint: CLOSED (present in tree).
- LM Studio v1 field names: CLOSED for list/load/chat; `/v1/messages` on LM Studio: OPEN. LM Link semantics: OPEN. Jan port/auth, KoboldCpp perf fields: OPEN (not cloned).
