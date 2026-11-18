# Editor isolation: spike analysis (T-11)

_Status: **analysis complete, measurement NOT run.** The decision rule in
`docs/migration/qt-to-tauri/implementation-roadmap.md` §2 (T-11) requires
measurements (≤ 40 MiB PSS and ≤ 5 ms p95 typing latency over the shared
webview). Those numbers do not exist, so **no decision is taken** and the
current design (Monaco in the application webview) stands. This document says
what the options are, what the spike must measure, and what the current
exposure is._

## 1. What the Qt shell had, and what the Tauri shell has

| | Qt shell (M1) | Tauri shell (M1.5, today) |
|---|---|---|
| Editor host | separate Qt WebEngine page (`axiom://editor/`) inside its own renderer process | same webview and same content process as the shell UI |
| What the editor page can do | only the 7 QWebChannel slots: it can save a document the shell gave it | call any of the host's 11 commands; through `daemon_request`, any protocol method a client may issue (except `initialize`/`shutdown`) |
| Crash blast radius | renderer only; shell survives, reloads the editor (M1 `tst_smoke_editor`) | the whole page dies; the host reloads it (T-08) and the UI rebuilds from daemon state + host snapshots |
| Cost per message | QWebChannel serialisation over the transport | none: direct function calls (the bridge-1 message set is an in-process interface) |

So the shared webview **gains latency** and **loses isolation** relative to Qt.
Both are consequences of one decision (architecture.md §3.3).

## 2. Options

**A. Keep Monaco in the application webview (current).**
- Pros: no IPC hop on the hot path; one content process (measured 282–307 MiB
  PSS total, cheaper than Qt's 446–494 MiB); simplest build.
- Cons: the editor shares the UI's privileges; a Monaco crash takes the UI;
  a future extension sandbox has no boundary to reuse.

**B. Monaco in a sandboxed `<iframe>` of the same webview.**
- Both documents share the process and the JS realm boundary is only the
  `iframe`'s (a same-origin iframe is *not* a security boundary; a
  `sandbox`ed cross-origin one is, but then the editor cannot be given a narrow
  capability set either — it simply loses all host access and must talk over
  `postMessage` to the parent, which *does* have the privileges). This is the
  Qt model, minus the process isolation: **it does not restore the security
  property and adds message serialisation.** Not recommended on its own.

**C. Monaco in a second webview (second content process).**
- Pros: restores crash isolation and lets the editor window carry a *narrower
  capability* (only the editor-bridge commands, e.g. snapshot + save request);
  restores the Qt model's trust boundary.
- Cons: one more engine process (memory), a real IPC hop per message
  (`openDocument` with a whole file's text, `saveRequested` with a whole file's
  text, cursor/selection at ≤ 20 Hz, snapshots ≤ 1 Hz), and window/layout
  complexity (a child webview must be positioned and resized with the layout,
  and on Linux a second webview means a second GTK window or an embedded one).

## 3. What the spike must measure (not run)

1. **Memory**: PSS of the whole shell tree with a document open, A vs C.
   Budget: C may cost ≤ 40 MiB more than A.
2. **Typing latency**: keystroke → pixels, 200 real keystrokes, p95.
   Budget: C may add ≤ 5 ms to p95 (A measured 18.8 ms p95 on Xvfb).
3. **Large-file open**: a 5 MiB file open → editor rendered (serialisation cost
   of `openDocument`).
4. **Crash isolation**: kill the editor's content process; the shell UI must
   survive with its tabs, and only the editor area reloads — the property Qt had.
5. **Capability narrowing**: prove the editor webview's capability file grants
   no `daemon_request` (only snapshot/save-request style commands).

A/B/C on the same machine, same file, same build profile, using
`apps/desktop-tauri/bench/run.py`'s Xvfb + XGetImage sampling for (2) and
`psutil` PSS for (1).

## 4. Why it was not run in this session

It needs a second working webview wired into the layout, a second capability
file, and a per-message protocol — i.e. most of a working day, and the result
only *might* change a design that already passes every parity and performance
budget. The exposure it would close (the editor sharing the UI's privileges) is
mitigated meanwhile by: no remote content, a strict CSP with no remote origins,
bundled-only assets, and a command surface that cannot touch the filesystem or
spawn processes directly (`src-tauri/tests/surface.rs` enforces the latter).

## 5. Recommendation

**Do not adopt isolation yet.** Run this spike only if either:
- M4 adds an extension host or an LSP client with a large, third-party code
  surface inside the editor's process (the privilege question becomes real), or
- a Monaco crash or hang is observed in the field that T-08's reload cannot
  paper over, or
- memory/latency budgets change such that the extra process is affordable and
  the isolation is worth it.

The `EditorPort`/`EditorListener` seam (ADR-0012 §4) is exactly what makes
option C a change to *plumbing*, not to the stores: `DocumentsStore` already
talks to the editor only through those message types.

## 6. Task status

T-11 remains **planned**: the analysis above is not its acceptance (which is a
measured decision). It is not on the critical path for M1.5.
