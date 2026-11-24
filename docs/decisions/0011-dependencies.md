# 0011 Dependencies
**Decision.** Permissive licences only (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, MPL-2.0 for unmodified crates); checked by `cargo deny` and `tool/check_licences.dart`. No GPL/AGPL code is read into the codebase (Zed's agent crates, Claude Squad: ideas only).
Fonts: OFL (Archivo, Spline Sans Mono), licence files shipped. Candidates and rejections: `../research/libraries.md`. A new dependency needs a reason in its commit message.
