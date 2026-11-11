# ADR-0002: Qt and Rust run in separate processes

Status: Accepted (2026-11-11)

## Context
Rust services and a Qt GUI can be combined in-process through FFI (CXX-Qt
0.10, qmetaobject-rs, hand-written C ABI) or kept in separate processes with
RPC. Research: `docs/research/qt-rust.md` §2–4.

## Decision
The shell (`axiom`, C++/QML) and the backend (`axiomd`, Rust) are separate
processes. The shell contains **no Rust**; the daemon contains **no Qt**.
They communicate only through the protocol of ADR-0003. The shell's C++ is a
thin presentation layer: a JSON-RPC client, item models, and the editor
bridge.

## Why
1. The brief requires GUI, CLI, agents, tests and remote clients to share one
   service layer. A protocol boundary is that layer; FFI would give the GUI a
   private fast path that the others lack, and logic would leak into it.
2. Crash isolation both ways (requirement N3).
3. Build simplicity: no cxx-qt-build/CMake/Cargo interleaving, no Qt needed
   to build or test the backend, no Rust needed to iterate on QML.
4. CXX-Qt's upstream describes its API as still changing; qmetaobject-rs is
   passively maintained. Neither is a foundation to bet the product on.
5. Remote execution falls out naturally.

## Evidence
E1: a C++/QML shell spawned a Rust daemon, exchanged JSON-RPC over stdio,
read and wrote a file, ran a process with streamed stdout/stderr and a
structured spawn error. Whole scripted run ≈ 1 s after editor ready; all 12
checks passed on Qt 6.11.2 and 6.12.0, offscreen and on Wayland.

## Consequences
- Every GUI↔platform interaction is async and JSON-serialised. Acceptable:
  per-keystroke editing stays inside the editor surface; file tree and
  output streaming are coarse-grained. If profiling later shows a hot path
  (e.g. huge output streams), fix it with batching in the protocol, not FFI.
- Some glue is written twice (Rust types, C++ JSON access). Mitigated by
  generated schemas (ADR-0005) and by keeping C++ models thin.
- CXX-Qt remains permitted *only* by a future ADR, for a specific in-process
  need that RPC demonstrably cannot meet.
