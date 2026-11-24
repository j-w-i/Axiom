# 0002 A Rust host process beside a Flutter client
**Decision.** All non-visual work runs in one Rust process that outlives the window.

**Evidence** (`prototypes/p1-ipc`, `p2-engine-lang`; M4 Pro, macOS 27, 2026-11-24)
| | Rust | Dart AOT |
|-|-|-|
| process start to first line, p50 | 2.8 ms | 43 ms |
| idle resident memory | 2 MB | 17 MB |
| literal search, 5.6 k files, warm | 31 ms | 56 ms |
| regex, gitignore, 5.3 k files | 34 ms | (no equivalent library) |
| process groups, PTY, rlimits, Landlock, Seatbelt, job objects | crates or direct syscalls | would need FFI for each |

In-process FFI was **not** faster than a socket: 1.5 M events/s by port vs 1.5 M by socket with the same JSON. So isolation is free.

**What the evidence does not say.** Raw speed is not the reason: Dart is within 2× on search. The reasons are sandboxing and process
control (SI-7), crash isolation of untrusted parsing, the Rust ecosystem (grep, tree-sitter, rmcp, ACP SDK, Codex's `apply-patch`),
and a GUI-free CLI.
**Alternatives.** All Dart: simplest, but every security primitive becomes FFI. FFI in-process: a parser bug takes the window down, sessions die with it.
TypeScript host: best provider code to reuse (Pi `ai`, MIT), but a 40–80 MB runtime, weak sandboxing, and a third language.
**Trade-off.** Two languages and a wire to maintain. **Risk.** Protocol drift; mitigated by generated types and contract tests.
**Revisit.** If wire maintenance exceeds its worth, or Dart gains process groups, PTY and sandbox APIs.
