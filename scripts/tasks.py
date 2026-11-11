#!/usr/bin/env python3
"""Task board for docs/tasks/**. Task files are Markdown with a small
front-matter block; the `status:` line there is the single source of truth.

  scripts/tasks.py                 list all tasks with status
  scripts/tasks.py ready           tasks that can be started now
  scripts/tasks.py next            the recommended next task (first ready by `order`)
  scripts/tasks.py set <ID> <status> [note]   update a task's status
  scripts/tasks.py check           validate ids, dependencies and statuses (used by check.sh)

Statuses: planned, in-progress, implemented, tested, verified, blocked.
"Ready" is derived: status == planned and every dependency is tested or verified.
"""
import pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "docs" / "tasks"
STATUSES = ["planned", "in-progress", "implemented", "tested", "verified", "blocked"]
DONE = {"tested", "verified"}


def load():
    tasks = {}
    for f in sorted(ROOT.rglob("*.md")):
        text = f.read_text(encoding="utf-8")
        m = re.match(r"---\n(.*?)\n---\n", text, re.S)
        if not m:
            continue
        meta = {}
        for line in m.group(1).splitlines():
            if ":" in line:
                k, v = line.split(":", 1)
                meta[k.strip()] = v.strip()
        if "id" not in meta:
            continue
        deps = [d.strip() for d in meta.get("depends_on", "").strip("[]").split(",") if d.strip()]
        tasks[meta["id"]] = dict(meta, deps=deps, file=f, order=int(meta.get("order", "999")))
    return tasks


def is_ready(t, tasks):
    return t["status"] == "planned" and all(tasks[d]["status"] in DONE for d in t["deps"] if d in tasks)


def check(tasks):
    errors = []
    for tid, t in tasks.items():
        if t.get("status") not in STATUSES:
            errors.append(f"{tid}: invalid status {t.get('status')!r}")
        if t["file"].stem != tid:
            errors.append(f"{tid}: file name {t['file'].name} does not match id")
        for d in t["deps"]:
            if d not in tasks:
                errors.append(f"{tid}: unknown dependency {d}")
    seen, stack = set(), set()

    def visit(tid):
        if tid in stack:
            errors.append(f"dependency cycle through {tid}")
            return
        if tid in seen or tid not in tasks:
            return
        stack.add(tid)
        for d in tasks[tid]["deps"]:
            visit(d)
        stack.discard(tid)
        seen.add(tid)

    for tid in tasks:
        visit(tid)
    return errors


def main(argv):
    tasks = load()
    cmd = argv[1] if len(argv) > 1 else "list"
    ordered = sorted(tasks.values(), key=lambda t: (t["order"], t["id"]))
    if cmd == "list":
        for t in ordered:
            shown = "READY" if is_ready(t, tasks) else t["status"]
            print(f"{t['id']:<18} {shown:<12} {t.get('title', '')}")
    elif cmd in ("ready", "next"):
        ready = [t for t in ordered if is_ready(t, tasks)]
        if not ready:
            active = [t["id"] for t in ordered if t["status"] in ("in-progress", "implemented")]
            print("No ready tasks." + (f" Unfinished: {', '.join(active)}" if active else ""))
            return 0
        for t in ready[:1] if cmd == "next" else ready:
            print(f"{t['id']:<18} {t['file'].relative_to(ROOT.parent.parent)}  — {t.get('title', '')}")
    elif cmd == "set" and len(argv) >= 4:
        tid, status = argv[2], argv[3]
        if tid not in tasks or status not in STATUSES:
            print(f"usage: tasks.py set <ID> <{'|'.join(STATUSES)}> [note]", file=sys.stderr)
            return 2
        f = tasks[tid]["file"]
        text = f.read_text(encoding="utf-8")
        text = re.sub(r"(?m)^status:.*$", f"status: {status}", text, count=1)
        note = " ".join(argv[4:])
        text = re.sub(r"(?m)^status_note:.*$", f"status_note: {note}", text, count=1)
        f.write_text(text, encoding="utf-8")
        print(f"{tid} -> {status}")
    elif cmd == "check":
        errors = check(tasks)
        for e in errors:
            print("task board:", e, file=sys.stderr)
        print(f"{len(tasks)} tasks, {len(errors)} problems")
        return 1 if errors else 0
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
