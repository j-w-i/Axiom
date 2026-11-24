# The archived Axiom v1

On 2026-11-24 the project was reset: `main` became an orphan branch for a new
product. Nothing was deleted and no remote ref was force-pushed; GitHub branches
were renamed.

| What | Where |
|------|-------|
| v1 `main`, complete history | branch `archive/axiom-v1`, tag `axiom-v1-final` (`8e0320c`) |
| Uncommitted work found in the tree (20 files + 1 untracked) | branch `archive/axiom-v1-ui-001-wip` (`379e3e9`), unreviewed |
| v1's feature branches | `archive/v1/task/*`, `archive/v1/qa/*` (all were already merged into v1 `main`) |
| v1's older milestones | tags `qt-shell-final`, `tauri-shell-final` |
| Local-only review branches | `claude/trusting-bardeen-5dc7a3`, `review/rust-fixes` (owner's machine only; also in the bundle) |
| Every ref above in one file | `~/Axiom-v1-archive/axiom-v1-all-refs-2026-11-24.bundle` (owner's machine; test-restored, 357 commits) |
| Untracked build leftovers | `~/Axiom-v1-archive/leftovers/` (owner's machine; regenerable, 4.1 GB) |

v1 may be read for research. Its code, dependencies, decisions and design system
are not inputs to the new Axiom.
