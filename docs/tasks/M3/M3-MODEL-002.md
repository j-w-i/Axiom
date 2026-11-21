---
id: M3-MODEL-002
title: OpenAI Chat adapter, capability profiles, keychain credentials, agent/models and agent/configureProvider
status: tested
status_note: 2026-11-21; see handoff
depends_on: [M3-MODEL-001]
order: 403
scope: crates/agent/, crates/axiomd/, crates/store/ (settings keys only), deny.toml
---

# M3-MODEL-002: OpenAI Chat adapter, capability profiles, keychain credentials, provider methods

## Objective
An OpenAI Chat Completions adapter (streamed, tool calls by index with argument fragments,
`stream_options.include_usage`), per-kind/model capability profiles with overrides, a credential
store over `keyring` with the `apiKeyEnv` alternative, provider configurations in settings
(`agent.providers`, no secrets), and the daemon serving `agent/models` and
`agent/configureProvider`.

## Context
Epic 3.1. ADR-0015 §1, §3, §4.

## Implementation requirements
1. OpenAI adapter: `POST {baseUrl}/chat/completions`; bearer key optional (local servers);
   `[DONE]` terminator; parallel tool calls by `index`; finish reasons mapped; servers that
   send whole arguments in one chunk handled.
2. Profiles: built-in defaults for `anthropic`, `openaiChat` (api.openai.com) and known local
   servers detected by base URL/port (Ollama 11434, llama.cpp 8080, LM Studio 1234); unknown →
   `unknown`. `models` lists via `GET /v1/models` (OpenAI-style) or a configured list.
3. `Credentials` trait with `KeyringCredentials` and an in-memory test double; key names
   `axiom/<provider id>`. `configureProvider` with a key stores it and the settings row gets
   `hasKey: true` only.
4. Loopback `http://` base URLs are allowed (local servers); every other URL must be HTTPS.

## Acceptance criteria
1. OpenAI fixtures (text; one tool call over several chunks; two parallel calls; Ollama-style
   whole-argument chunk; error JSON) produce the expected events.
2. `agent/configureProvider` then `agent/models` round trip in an axiomd integration test, with
   the key in the test credential store and absent from the settings table and logs.
3. Keyring on Linux: a manual or CI check stores and reads a key where a Secret Service is
   available; where none is, the error says to use `apiKeyEnv`.

## Tests
`cargo test -p axiom-agent openai_`; `cargo test -p axiomd agent_models`; `scripts/check.sh`.

## Failure conditions
A secret reaches settings, SQLite, a transcript or a log line.

## Documentation updates
`agents.md` §Model providers (profiles table), `security.md` §Credentials.

## Handoff
Which keyring backends were exercised on which OS.
