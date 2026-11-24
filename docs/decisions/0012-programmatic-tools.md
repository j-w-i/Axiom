# 0012 Programmatic tool calling (direction; decided in M4)
**Direction.** A `ScriptHost` trait; scripts run in a separate process with no filesystem, no network and one pipe to the engine, with the caller's capabilities. Candidates: Monty (Python subset in Rust, suspends at host calls) and QuickJS via rquickjs.
Decided by a 20-task evaluation, because published token savings are vendor-reported (`../research/sandboxing.md` §9). V8 rejected for its patch treadmill.
