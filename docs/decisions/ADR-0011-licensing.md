# ADR-0011: Licensing

Status: Accepted (2026-11-11). Not legal advice; a legal review is required
before the first binary release (tracked in M7).

## Decision
1. Axiom's own code is dual-licensed **MIT OR Apache-2.0**.
2. Qt is used under **LGPL-3.0**, dynamically linked, unmodified. Binary
   distributions must ship Qt as replaceable shared libraries, include the
   licence texts and Qt/Chromium third-party notices, and offer the
   corresponding Qt source (or a written offer / link to the exact source).
3. **Only LGPL-available Qt modules may be used.** Allowed today:
   `Core, Gui, Qml, Quick, QuickControls2, QuickDialogs2, Svg, WebEngineCore,
   WebEngineQuick, WebChannel, WebSockets, Network, Concurrent, Test,
   QuickTest`. **Forbidden** (GPL-only on the open-source path per the Qt
   6.12 licensing page, see `docs/research/qt-rust.md`): Canvas Painter,
   CoAP, Graphs, gRPC, HTTP Server, Lottie, MQTT, Network Authorization,
   QML Compiler (`qmlsc` — the open `qmlcachegen`/`qmltc` are fine), Quick 3D,
   Quick 3D Physics, Quick Timeline, Virtual Keyboard, Wayland Compositor.
   Adding any Qt module not on the allowed list requires checking its
   licence and amending this ADR.
4. The QML inspection probe (M6) includes private Qt headers. It is built
   and shipped as a separate shared library under the same LGPL terms of use;
   confirm in the M7 legal review.
5. Rust and npm dependencies must be under MIT, Apache-2.0, BSD-2/3-Clause,
   ISC, Zlib, Unicode-3.0, MPL-2.0 (unmodified), or CC0. Enforced by
   `cargo deny` (M1-CI-001) and a licence check on the editor bundle.
   No GPL/AGPL code is copied into the repository (e.g. from Qt Creator,
   GammaRay, Zed's GPL crates); such tools may be *invoked* as separate
   processes if the user has them.
6. Extensions: Axiom will use Open VSX, never the Microsoft Marketplace, and
   will not bundle Microsoft-proprietary extensions.
7. Apple SDK content is never redistributed. The xtool provider guides the
   user through supplying their own `Xcode.xip`; the legal considerations are
   summarised in `docs/research/xtool.md` and shown to the user at setup.

## Consequences
- Static linking of Qt and single-file executables are off the table.
- macOS hardened runtime / notarisation must be reconciled with LGPL
  "replaceable library" obligations (open question for M7).
