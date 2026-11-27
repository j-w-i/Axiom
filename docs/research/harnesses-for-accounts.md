# DeepSeek Harness, Pi and OpenCode, for running an account's model

Read 2026-11-27 from the projects' own repositories, npm's registry and opencode.ai, for decision 0023. Nothing was installed, built or run. Grades: **[C]** read in code, **[D]** their own documentation, **[3]** others only.

## DeepSeek Harness (`dsh`)
- MIT, `github.com/deepseek-ai/deepseek-harness` [D]. npm `@deepseek-ai/dsh`, `latest` 0.2.2-rc.5; the repository is tagged up to `dsh-v0.2.3-alpha.2` and calls itself a developer preview with breaking changes to come [D]. Needs Node; 6.2 MB unpacked before its dependencies, which were not counted.
- Profiles under `$DSH_HOME/profiles/` (`~/.dsh` unless set); `web`, `headless`, `sdk`, `sdk-minimal`, `acp` begin by themselves [D `apps/cli/README.md`].
- `--profile headless`: one task (arguments or standard input), the answer on standard output, `--json` for events (no schema found), `--session-id` to go on, `--approval-policy ask|never`, 0 done, 1 not [D `docs/cli-help.md`, `packages/bundle/headless/README.md`]. MCP servers only from the profile's files.
- `--profile acp`, standard streams: ACP v1 and `session/list`, `session/resume`, `session/close`; `session/new` takes stdio and streamable-HTTP `mcpServers`; it sends `session/request_permission` (allow or reject, once); no `session/load`; model and `reasoning_effort` by `session/set_config_option`; `session/cancel`. No plans, terminals, client files, fork [D `packages/acp/acp/README.md`].
- Configuration: YAML patches `$DSH_HOME/cordis.patch.yml` and per profile; for ACP `provider` and `model` hold for every session [D]. The DeepSeek route reads `DEEPSEEK_API_KEY` (or the `apiKeyEnv` named) and gives it only to its base URL's origin [D `packages/llm/llm-deepseek-api-key`]. Keys are looked for in the environment, then `<cwd>/.env`, then `$DSH_HOME/.env` [D]. Other providers through pi-ai: `anthropic`, `openai`, `openai-completions` with `baseURL` and `apiKeyEnv` [D `packages/llm/llm-pi-ai`]; Ollama, LM Studio and OpenRouter would be the last.
- Its own sandbox: bubblewrap or Landlock on Linux, Seatbelt on macOS, a restricted token on Windows (partial there); fails closed with `SANDBOX_UNAVAILABLE` [D `packages/sandbox/sandbox-local`]. Tools: bash, files, patch, search, todo, plan, subagents [D].
- `DSH_TELEMETRY_DISABLED` turns telemetry off [C `packages/boot/app-boot/src/profile-context.ts`]; it asks `api.deepseek.com/models` for models [D].

## Pi
- MIT, `github.com/earendil-works/pi`, npm `@earendil-works/pi-coding-agent` 1.6.1, 24.9 MB unpacked, 22 dependencies [D].
- `--mode rpc`: JSON lines (split on LF only); commands `{type, id}` answered by `response`, events without an id; `prompt`, `steer`, `follow_up`, `abort`, `new_session`, `fork`, `compact`, `set_model {provider, modelId}`, `set_thinking_level`, `get_available_models`; `agent_settled` when it will not go on by itself [D `docs/rpc.md`, `rpc-commands.md`].
- Keys from the provider's variable (`ANTHROPIC_API_KEY`, …) or `auth.json` in its folder; `--api-key` would put one in the arguments. Its folder is `~/.pi/agent` unless `PI_CODING_AGENT_DIR`; sessions `PI_CODING_AGENT_SESSION_DIR` [D `environment-variables.md`].
- MCP over stdio and streamable HTTP, from `mcp.json` in its folder and, after the project is trusted, `.pi/mcp.json` [D `docs/mcp.md`].
- No ACP of its own; `svkozak/pi-acp` (MIT, others') wraps `--mode rpc` [3].
- It does not ask before a tool call, and has no sandbox: "isolation is on you" [D `docs/security.md`].
- `PI_OFFLINE` stops its requests to `pi.dev` at start; analytics off by default [D].

## OpenCode
- MIT, `github.com/anomalyco/opencode` (was `sst/opencode`), v1.19.6 (2026-11-17) [D]. npm `opencode-ai` is a shim over one package per system; a release is 64 to 82 MB compressed [D].
- `opencode acp`: session create, load, resume, fork, list, close; `mcpServers` stdio and HTTP; permission requests (once, always, reject); model, effort and mode as configuration options; ends when its input does [D opencode.ai/docs/acp, C `packages/cli-rs/crates/acp`]. Which protocol version is not said.
- Keys in `opencode.db` (SQLite) under `XDG_DATA_HOME`, earlier `auth.json`; `{env:VAR}` in configuration; Ollama and LM Studio found at their ports; `OPENROUTER_API_KEY` [D]. Moved by `OPENCODE_CONFIG_DIR`, `OPENCODE_CONFIG`, `OPENCODE_CONFIG_CONTENT`, `XDG_*`; a system-wide managed configuration wins over all [D].
- Downloads by itself unless told not to: updates (`OPENCODE_DISABLE_AUTOUPDATE`), language servers (`OPENCODE_DISABLE_LSP_DOWNLOAD`), lists of models (`OPENCODE_DISABLE_MODELS_FETCH`), plugins (`OPENCODE_DISABLE_DEFAULT_PLUGINS`); `OPENCODE_DISABLE_PROJECT_CONFIG` ignores a repository's own [D cli.mdx].
- No sandbox of its own found.

## Open
dsh's `--json` events and its permission preset under ACP; whether Ollama and OpenRouter need a route of their own in dsh; Pi's whole event list; OpenCode's ACP version and default permissions; what each downloads when first started.
