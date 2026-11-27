# How T3 Code runs agents, a browser and devices

Read 2026-11-26 from `pingdotgg/t3code` at `23057ddd` (0.1.12), source only: nothing was installed, built or run. Asked for by the owner after "I no longer want Axiom to be a harness" (state.md), for a decision after M4. Grades: **[C]** read in code, **[D]** read in its docs, **[I]** inferred.

## Agent loop
No model loop of its own. Eleven drivers (`apps/server/src/provider/builtInDrivers.ts` [C]): Codex (app-server, one per many threads), Claude (`claude-agent-sdk` `query()` with `canUseTool`), Cursor, Grok, fx, OpenCode (`opencode serve` over HTTP with a password), Antigravity, Kiro, Pi (`--mode rpc`), Muse, and any ACP agent (stdio, v2 then v1). Models no harness provides come in through one: Codex `--oss --local-provider ollama`, OpenCode's providers, Pi's `--provider`, or an ACP command, whose docs name `dsh --profile acp` (`docs/user/providers-acp.md` [D]). Direct model calls are only tool-less "text generators" for titles, branch names, commit and PR text (`apps/server/src/textGeneration/` [C]). Sandboxing is each harness's own; on Linux `systemd-run` scopes limit memory [C]. Default permission mode is Full access, and for ACP agents T3 approves by itself in Auto [D].

## Its features for every agent
One MCP server in the host, 92 tools (`apps/server/src/mcp/toolkits/` [C]). Per session a 32-byte token, SHA-256 kept, 24 h idle expiry, capabilities (`preview`, `orchestration`, `device`, …), read-only sessions get read-only tools, a delegated child stays within its parent (`McpSessionRegistry.ts`, `McpToolAccess.ts` [C]). Four tools listed up front, the rest unlocked by a search tool (`McpToolExposure.ts` [C]). Overlap with a harness's own tools is handled by rules in prose added to its instructions, not by removing tools (`orchestrationInstructions.ts` [C]). Delivery: HTTP with a header for Claude and Codex; a stdio bridge for ACP, with the token in the agent's environment (`T3_ACP_MCP_AUTHORIZATION` [C]), which SI-3 forbids here.

## Browser
Desktop: Electron webviews with a picking preload and a CDP relay. Server: headless Chromium through `playwright-core`, its sandbox on, streamed as screencast frames with one in flight (`ServerBrowserStream.ts` [C]). Agents: `preview_snapshot` (accessibility tree with refs), `preview_act`, evaluate (64 KB), wait, resize, dark mode, recordings (webm made in the client). Holding a tab is tracked with epochs; the person takes control by a key (`SessionControl.ts` [C]). http and https only; no host list [C].

## Devices
`agent-device` 0.21.41 (Callstack, MIT) for control and `expo-device-hub` 0.26.1 (MIT) for streams, pinned (`device/deviceToolVersions.json` [C]); the host proxies a fixed set of stream paths; `device_open` gives the agent a bound command through a shim on its `PATH`. Remote Macs over system ssh with `-F none` [C].

## Terminals, git
`node-pty` in the server, 5 000 lines or 8 MiB kept; libghostty compiled to WASM draws in the client [C]. Git is the git CLI; worktrees by `git worktree add`; pull requests through GitHub's API with a token from settings, `GH_TOKEN` or `gh auth token` [C] (the last reads another program's credential: 0007).

## What Axiom could take, as ideas
1. No loop of its own: models no harness provides through dsh over ACP, which Axiom speaks; a small tool-less client for titles and commit messages, or those through dsh too.
2. One MCP server of Axiom's for every harness, from `host/src/mcp.rs`: capabilities per session, few tools up front, children within their parent; the credential on a socket or an inherited descriptor, never in the environment.
3. Only tools a harness lacks (0017); rules in prose for the rest.
4. The browser in the host as T3's server does it: screencast with acknowledgement, accessibility refs, an epoch for who holds it (0021).
5. Devices through the same two MIT programs, if wanted.
