# 0008 Capabilities, one spawn path, and containment by one owner
**Decision.** Authority is a set of capabilities granted by the user's policy, intersected down every parent→child edge (SI-1). One module starts processes and requires a sandbox profile (SI-7);
failure to build a sandbox is an error (SI-8). Exactly one of Axiom, the harness or nobody contains a session, and it is displayed (SI-26), because **nested Seatbelt fails on macOS 27** (tested).
macOS Seatbelt, Linux bubblewrap + seccomp + Landlock, Windows restricted: no auto-run until a verified backend exists; WSL2 otherwise.
**Evidence.** `../security/threat-model.md`, `../research/sandboxing.md`. **Risk.** Windows is the schedule risk; Microsoft MXC's unavoidable telemetry needs its own decision.
