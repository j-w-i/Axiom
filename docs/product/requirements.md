# Product requirements

Source: the owner's brief of 2026-11-24 and the owner's notes since. `M` is the milestone that must
satisfy it. A requirement is met when its acceptance test passes, not when code exists.

## Connections and models
| ID | Requirement | M |
|----|-------------|---|
| PR-1 | Provider, model, connection, harness and effort are separate and separately selectable per agent | 2 |
| PR-2 | API-key, plan sign-in, and installed-harness lanes are distinct; the lane and account in use are always visible; Axiom never switches lane silently | 2 |
| PR-3 | Five wire codecs (Anthropic Messages, OpenAI Responses, OpenAI Chat, Gemini, Bedrock Converse) with per-provider quirk profiles as data; provider-native content survives replay | 2 |
| PR-4 | **Several accounts per service** (owner, 2026-11-24): any number of linked connections to the same service, each with its own identity, limits and state; the header shows a dropdown per service | 2 (UI in M1) |
| PR-5 | Local servers (Ollama, LM Studio, llama.cpp, then generic OpenAI-compatible) are discovered, inspected and diagnosed; Axiom never starts a server or loads a model unasked | 2 |
| PR-6 | Only authorised subscription routes (`research/subscription-auth.md`); each carries dated policy metadata and can be switched off remotely, restrict-only | 3 |
| PR-7 | No automatic rotation between accounts of one service. Moving to another account is a person's act or a routing rule they wrote, where the provider's terms allow it | 6 |

## Execution
| ID | Requirement | M |
|----|-------------|---|
| PR-10 | Native engine: iterative loop, parallel tools, steering, queueing, interrupt, checkpoint, resume, compaction, branching, model switch | 2–4 |
| PR-11 | Delegated harnesses through structured interfaces; capabilities are declared, never faked | 3 |
| PR-12 | The executing harness is never substituted or misrepresented | 2 |
| PR-13 | The engine runs, and is fully testable, without the GUI; a CLI drives it | 2 |
| PR-14 | Sessions outlive the GUI: quitting the window does not stop work unless the person says so | 2 |
| PR-15 | Native tools designed for agent effectiveness, with measured context cost and latency | 2–4 |
| PR-16 | Deferred tool discovery; programmatic tool calling in a capability-limited sandbox | 4 |

## Orchestration
| ID | Requirement | M |
|----|-------------|---|
| PR-20 | Agents with different harness, model, connection, tools, access and folder in one session | 5 |
| PR-21 | Delegation across harnesses; delivery of results exactly once | 5 |
| PR-22 | Writers are isolated (worktree per writer, created at first send); conflicts are detected before merge | 5 |
| PR-23 | Every agent's task, state, model, harness, parent, activity, files, time, context and usage are visible; inspect, steer, pause, stop, reassign | 5 |
| PR-24 | One queue of everything that needs a person, across agents | 5 |
| PR-25 | Task graphs with gates; patterns: supervisor, pipeline, competition, independent verification | 5 |

## Usage
| ID | Requirement | M |
|----|-------------|---|
| PR-30 | Every usage figure carries its provenance; unknown is shown as unknown | 1 (component), 6 |
| PR-31 | Budgets and warnings per account, agent and session; routing rules are explicit and explain each choice | 6 |

## Security, privacy
`../security/threat-model.md` is normative: invariants SI-1…SI-26.

## Experience
| ID | Requirement | M |
|----|-------------|---|
| PR-40 | Original design language; dark first, light equal; no default Material | 1 |
| PR-41 | Everything works by keyboard; contrast ≥ 4.5:1 for text; reduced motion honoured | 1 |
| PR-42 | Long sessions stay at 120 Hz budgets (`../engineering/performance.md`) | 1–7 |
| PR-43 | Workspace state survives restarts and crashes | 2 |
| PR-44 | **Development never opens windows on the owner's desktop** (owner, 2026-11-24) | all |

## Extensibility
PR-50 extensions declare capabilities, versions and permissions; PR-51 import `AGENTS.md`,
`CLAUDE.md`, `GEMINI.md`, rules, `SKILL.md`, hooks and `mcpServers` from other harnesses (M6).
