# IBM Plex: source of these files

IBM's own, **unmodified** split WOFF2 files (`fonts/split/woff2/`, the `Latin1`
subset), copied byte for byte from the official release archives. They are not
subsetted, converted or otherwise changed (ADR-0013 §6: the licence reserves the
font name "Plex"). Licence: SIL Open Font License 1.1, [`OFL.txt`](OFL.txt)
(the release's `LICENSE.txt`, identical in both archives).

| Family | Release | Archive URL | Archive SHA-256 |
|--------|---------|-------------|-----------------|
| IBM Plex Sans | `@ibm/plex-sans@1.1.0` (2024-11-13) | https://github.com/IBM/plex/releases/download/%40ibm/plex-sans%401.1.0/ibm-plex-sans.zip (9,921,777 bytes) | `fb365d910566e6d199cc2c15579a7dd9a267128e18431a394ed81f1970c69200` (computed on download, 2026-11-17; GitHub publishes no digest for this asset) |
| IBM Plex Mono | `@ibm/plex-mono@2.5.0` (2026-06-11) | https://github.com/IBM/plex/releases/download/%40ibm/plex-mono%402.5.0/ibm-plex-mono.zip (6,940,652 bytes) | `6d23f01257663d8cc49a0d64c22ced630b79e0e2a0ac08a0da86e9a38bbc481c` (matches the digest GitHub publishes for the asset) |

| File | Bytes | SHA-256 |
|------|------:|---------|
| `IBMPlexSans-Regular-Latin1.woff2` | 20,984 | `b5ad7bd39f996144915f0ad9849a90183b27d8c28ad97ed98af5b1bebc51f6b1` |
| `IBMPlexSans-Medium-Latin1.woff2` | 21,960 | `b5610af04d0d4b5a14a621d96d974b993e945a065db1a8861918f69ef9321934` |
| `IBMPlexSans-SemiBold-Latin1.woff2` | 22,260 | `fff0ab3a88b0b4aa0b693e4f0201359a15183b08e3fa5696d1918d8f0ade8ad5` |
| `IBMPlexSans-Italic-Latin1.woff2` | 22,924 | `0a06b98143f3453b81f3c396241a01c6c4cff84c1a77bf0c75b18bd603018506` |
| `IBMPlexMono-Regular-Latin1.woff2` | 17,544 | `e8993d946649b9d01abb1ed06d574b19d8ea3e66b5c3948602db335c44c18e56` |
| `IBMPlexMono-Medium-Latin1.woff2` | 17,868 | `41201b658a328b9d00368215c2f1102770f80b15952ab82631e4006255e6365d` |
| `IBMPlexMono-Italic-Latin1.woff2` | 19,556 | `242f6417615190011bad9e6125180e356f9f9710186238784659927fbaf7ce76` |
| **Total** | **143,096** | |

The `unicode-range` of each face in `src/styles/fonts.css` is copied from IBM's own
CSS in the same archives (`css/ibm-plex-*-all.css`, the `Latin1` faces). The `Pi`
split (arrows, maths, U+2713) is not bundled: nothing in the shell or in the
console output it shows needs it; those characters fall back to the system face.
