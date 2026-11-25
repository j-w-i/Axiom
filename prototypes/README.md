# Prototypes

Throwaway code that answered one question each. Results are in the decisions named; nothing here ships.

| | Question | Answer |
|-|----------|--------|
| p1-ipc | Which wire, and is FFI worth it? | JSON lines over a local socket; FFI no faster (0003) |
| p2-engine-lang | Dart or Rust for the engine; what does durability cost? | Rust, for control more than speed (0002); SQLite WAL (0004) |
| p3-transcript | Can Flutter hold 120 Hz while agents stream? | Yes if block-incremental; whole-document parsing costs 260 ms a frame at 5 000 blocks (0010). **Opens a window: do not run on the owner's desktop** |
| p4-concepts | What should Axiom look like? | `docs/design/system.md`; fonts are not checked in (Google Fonts, OFL) |
