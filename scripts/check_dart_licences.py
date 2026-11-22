#!/usr/bin/env python3
"""Every Dart package the Flutter shell resolves (direct and transitive, dev included) must be
MIT, BSD, Apache-2.0 or Zlib (ADR-0011, ADR-0020 §10). Reads the resolved package config, so run
`flutter pub get` first. Packages that ship inside the Flutter SDK are BSD-3 (Flutter's licence).

    scripts/check_dart_licences.py [apps/desktop-flutter]
"""

import json
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse

ALLOWED = ("MIT", "BSD-3-Clause", "BSD-2-Clause", "Apache-2.0", "Zlib")

# The first clause that identifies the licence text. Order matters: Apache texts quote other
# licences, and BSD-3 contains BSD-2's clauses.
SIGNATURES = [
    ("Apache-2.0", re.compile(r"Apache License,?\s+Version 2\.0", re.I)),
    ("MIT", re.compile(r"Permission is hereby granted, free of charge, to any person obtaining", re.I)),
    ("BSD-3-Clause", re.compile(r"Neither the name of .{0,200}? nor the names of its\s+contributors", re.I | re.S)),
    ("BSD-2-Clause", re.compile(r"Redistributions in binary form must reproduce the above copyright", re.I)),
    ("Zlib", re.compile(r"This software is provided 'as-is', without any express or implied", re.I)),
]


def classify(text: str) -> str | None:
    for name, sig in SIGNATURES:
        if sig.search(text):
            return name
    return None


def main() -> int:
    app = Path(sys.argv[1] if len(sys.argv) > 1 else "apps/desktop-flutter").resolve()
    config = app / ".dart_tool" / "package_config.json"
    if not config.is_file():
        print(f"check_dart_licences: {config} not found (run flutter pub get)", file=sys.stderr)
        return 2
    packages = json.loads(config.read_text())["packages"]
    bad = []
    rows = []
    for p in packages:
        name = p["name"]
        uri = p["rootUri"]
        root = (config.parent / unquote(uri)).resolve() if not uri.startswith("file:") else Path(unquote(urlparse(uri).path))
        if root == app:
            continue  # the app itself
        in_sdk = "/flutter/packages/" in str(root) or "/flutter/bin/cache/" in str(root)
        files = sorted(root.glob("LICENSE*")) + sorted(root.glob("COPYING*"))
        lic = None
        if files:
            lic = classify(files[0].read_text(errors="replace"))
        if lic is None and in_sdk:
            lic = "BSD-3-Clause"  # the Flutter SDK's own licence
        rows.append((name, lic or "UNKNOWN", "sdk" if in_sdk else "pub"))
        if lic not in ALLOWED:
            bad.append((name, lic or "no recognisable licence", str(root)))
    width = max((len(r[0]) for r in rows), default=10)
    for name, lic, src in sorted(rows):
        print(f"{name:<{width}}  {lic:<13} {src}")
    if bad:
        print("\nNot allowed (ADR-0011): " + "; ".join(f"{n}: {l} ({r})" for n, l, r in bad), file=sys.stderr)
        return 1
    print(f"\n{len(rows)} Dart packages, all {', '.join(sorted(set(r[1] for r in rows)))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
