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
    # M2 (ADR-0014 §4).
    "axiom-env": {"axiom-protocol", "axiom-store"},
    "axiom-build": {"axiom-protocol", "axiom-exec", "axiom-env"},
    # M3 (ADR-0015 §2): the agent runtime calls services only through the tools axiomd registers.
    "axiom-agent": {"axiom-protocol", "axiom-store"},
    "axiomd": {
        "axiom-protocol", "axiom-rpc", "axiom-store", "axiom-workspace", "axiom-exec",
        "axiom-env", "axiom-build", "axiom-agent",
    },
    # The `axiom` command-line client (ADR-0015 §2, §6): a protocol client.
    "axiom-cli": {"axiom-protocol", "axiom-rpc"},
    # The GUI's daemon client (ADR-0012): protocol only.
    "axiom-desktop-bridge": {"axiom-protocol"},
    # Tauri shell host: bridge only (plus third-party crates).
    "axiom-desktop-tauri": {"axiom-desktop-bridge"},
    # Test helper binary (tests/helpers/emit): depends on nothing in the workspace.
    "emit": set(),
    # Test helper binary (tests/helpers/fake-mcp): an MCP server for the MCP client tests.
    "fake-mcp": set(),
    # Test helper binary (tests/helpers/fake-acp): an ACP agent for the ACP client tests.
    "fake-acp": set(),
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
