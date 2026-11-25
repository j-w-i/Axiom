# Axiom

An AI agent harness and multi-agent orchestration platform for the desktop
(macOS, Windows, Linux).

> The user chooses the intelligence, the execution environment, the tools, the
> authentication method and the orchestration strategy. Axiom makes them work
> together.

**Status: M2, under way.** Agents run: on keys and local servers with Axiom's own loop and
sandbox, or as Claude Code, Codex and ACP programs you have installed. Several of them work in one
session and can give each other work. What has been verified, and where, is in `docs/state.md`; it
has not had its independent security review.

```sh
scripts/package.sh   # builds the application with its host beside it
```

This repository was reset on 2026-11-24. The previous product of the same name
(an LLM-native development platform) is preserved on the branch
`archive/axiom-v1` and the tag `axiom-v1-final`; see `docs/archive.md`.

Licensed under MIT or Apache-2.0, at your option.
