#!/usr/bin/env python3
"""QML literal guard: views in apps/desktop/qml/Axiom/App must use Axiom.Ui
tokens, never colour literals or numeric font sizes (AGENTS.md, QML rules).

Fails on:
  - a hex colour inside a string literal ("#rgb", "#rrggbb", "#aarrggbb", ...)
  - `font.pixelSize:` or `font.pointSize:` followed by a number
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
APP_DIR = ROOT / "apps" / "desktop" / "qml" / "Axiom" / "App"

COLOUR = re.compile(r"""["']#[0-9a-fA-F]{3,8}["']""")
FONT = re.compile(r"font\.(?:pixelSize|pointSize)\s*:\s*\d")


def scan(path: pathlib.Path):
    problems = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        code = line.split("//", 1)[0]
        if COLOUR.search(code):
            problems.append(f"{path}:{n}: colour literal: {line.strip()}")
        if FONT.search(code):
            problems.append(f"{path}:{n}: numeric font size: {line.strip()}")
    return problems


def main(argv):
    base = pathlib.Path(argv[1]) if len(argv) > 1 else APP_DIR
    if not base.exists():
        print(f"qml guard: {base} does not exist; nothing to check")
        return 0
    problems = []
    for f in sorted(base.rglob("*.qml")):
        problems += scan(f)
    if problems:
        print("QML literal guard violations (use Axiom.Ui tokens):", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print("qml literal guard ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
