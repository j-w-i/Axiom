# IBM Plex: source of these files

IBM's own, **unmodified** TTF files (`fonts/complete/ttf/`), extracted byte for byte from the
same official release archives the Tauri shell's WOFF2 files come from
(`apps/desktop-tauri/src/assets/fonts/ibm-plex/SOURCE.md` at the tag `tauri-shell-final`; archive SHA-256 values re-checked on
download, 2026-11-22). Flutter cannot load WOFF2, and converting the web files would change them
(ADR-0013 §6: the licence reserves the font name "Plex"), so the complete TTFs are used.
Licence: SIL Open Font License 1.1, [`OFL.txt`](OFL.txt) (`fonts/complete/ttf/license.txt`,
identical in both archives).

| Family | Release | Archive SHA-256 |
|--------|---------|-----------------|
| IBM Plex Sans | `@ibm/plex-sans@1.1.0` | `fb365d910566e6d199cc2c15579a7dd9a267128e18431a394ed81f1970c69200` |
| IBM Plex Mono | `@ibm/plex-mono@2.5.0` | `6d23f01257663d8cc49a0d64c22ced630b79e0e2a0ac08a0da86e9a38bbc481c` |

| File | Bytes | SHA-256 |
|------|------:|---------|
| `IBMPlexMono-Italic.ttf` | 179756 | `a55368deb953f594ac18336a1029009ed820e2d9abbb57ad601f61e22bec072b` |
| `IBMPlexMono-Medium.ttf` | 174008 | `98fbd727aae340b236955879dabed4d991aac9e8e90b3b2a67ce4a59221cc97c` |
| `IBMPlexMono-Regular.ttf` | 173052 | `7c6fbddca4b700be918f5f6183d9bd4464fa427fe435f0b480d77fe2bb8c5a43` |
| `IBMPlexSans-Italic.ttf` | 207920 | `a9c6ef9942c49e49d11e11a6dacc0b3a087978757e9b22a06b8ac22a6400fb15` |
| `IBMPlexSans-Medium.ttf` | 202460 | `331c8639d7598b2cde62a911a71db195e30cb655cd6bdf2e324a7e984955f907` |
| `IBMPlexSans-Regular.ttf` | 200500 | `975dcda37d80f038dcd143c22e33ca2d97a0cc5a929aace1c749153b0fe1afa5` |
| `IBMPlexSans-SemiBold.ttf` | 202632 | `a20caf8286023a6a7a85e40b1d2a4ae9fc3e3b1f9eda8f4c542dd4986af67bb1` |
