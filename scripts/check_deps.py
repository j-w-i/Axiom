#!/usr/bin/env python3
"""Fails if the workspace has an internal dependency edge that
docs/architecture/modules.md (§Dependency matrix) does not allow.

Usage: scripts/check_deps.py            (reads `cargo metadata`)
       scripts/check_deps.py --metadata FILE   (use a saved metadata JSON; for tests)
"""
import json
import os
import subprocess
import sys

# crate -> internal crates it may depend on. Mirrors the matrix in modules.md.
# Dev-dependencies count too: a test-only edge is still an edge.
ALLOWED = {
    "axiom-protocol": set(),
    "axiom-rpc": {"axiom-protocol"},
    "axiom-store": set(),
    "axiom-workspace": {"axiom-protocol"},
    "axiom-exec": {"axiom-protocol"},
    "axiomd": {"axiom-protocol", "axiom-rpc", "axiom-store", "axiom-workspace", "axiom-exec"},
    # Test helper binary (tests/helpers/emit): depends on nothing in the workspace.
    "emit": set(),
}


def load_metadata(argv):
    if len(argv) >= 3 and argv[1] == "--metadata":
        with open(argv[2], encoding="utf-8") as f:
            return json.load(f)
    env = dict(os.environ)
    env["PATH"] = os.path.expanduser("~/.cargo/bin") + os.pathsep + env.get("PATH", "")
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        check=True, capture_output=True, text=True, env=env,
    ).stdout
    return json.loads(out)


def main(argv):
    meta = load_metadata(argv)
    packages = {p["name"]: p for p in meta["packages"]}
    errors = []
    for name, pkg in sorted(packages.items()):
        if name not in ALLOWED:
            errors.append(
                f"crate `{name}` is not in the dependency matrix: add it to "
                f"docs/architecture/modules.md and to ALLOWED in scripts/check_deps.py"
            )
            continue
        for dep in pkg["dependencies"]:
            # `--no-deps` still lists every declared dependency; keep workspace-internal ones.
            if dep["name"] in packages and dep["name"] != name:
                if dep["name"] not in ALLOWED[name]:
                    kind = dep.get("kind") or "normal"
                    errors.append(
                        f"forbidden dependency edge: {name} -> {dep['name']} ({kind}); "
                        f"allowed for {name}: {sorted(ALLOWED[name]) or 'none'}"
                    )
    if errors:
        print("dependency matrix violations:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print(f"dependency matrix ok ({len(packages)} crates)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
