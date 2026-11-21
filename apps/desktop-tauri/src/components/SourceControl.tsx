// The Source Control page of the side bar (M4-GIT-002): the branch, and the changes grouped as
// Conflicts, Staged, Changes and Untracked. A row is a button: Enter opens its diff, Shift+Enter
// the file; ↑/↓/Home/End move between rows (one Tab stop). Read-only: no staging or commits in M4.

import { useRef, useSyncExternalStore, type KeyboardEvent } from "react";
import type { GitFile } from "../daemon/protocol.gen";
import { groups, markOf, type GitStore } from "../state/git";
import { problemTarget } from "./Problems";
import "../styles/components/scm.css";

const GROUP_LABEL = { conflicts: "Conflicts", staged: "Staged", changes: "Changes", untracked: "Untracked" } as const;

export function SourceControlSide({ git, onOpenFile }: { git: GitStore; onOpenFile: (path: string) => void }) {
  const s = useSyncExternalStore(git.subscribe, git.getSnapshot, git.getSnapshot);
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  if (!s.status) return <p className="scm__empty">Reading the repository…</p>;
  if (!s.status.repo) return <p className="scm__empty">{s.status.reason ?? "This folder is not in a Git repository."}</p>;
  const g = groups(s.status);
  const sections = (Object.keys(GROUP_LABEL) as (keyof typeof GROUP_LABEL)[]).filter((k) => g[k].length > 0);
  const flat: { key: string; f: GitFile; staged: boolean }[] = sections.flatMap((k) => g[k].map((f) => ({ key: `${k}:${f.path}`, f, staged: k === "staged" })));
  const onKey = (e: KeyboardEvent<HTMLButtonElement>, i: number, f: GitFile) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) onOpenFile(f.path);
      else git.openDiff(f.path);
      return;
    }
    const to = problemTarget(e.key, i, flat.length);
    if (to === null) return;
    e.preventDefault();
    refs.current[to]?.focus();
  };
  let index = -1;
  return (
    <div className="scm" data-testid="scm">
      <p className="scm__branch">
        {s.status.branch ?? `Detached at ${s.status.head ?? "?"}`}
        {(s.status.ahead ?? 0) > 0 && <span className="scm__ab"> ↑{s.status.ahead}</span>}
        {(s.status.behind ?? 0) > 0 && <span className="scm__ab"> ↓{s.status.behind}</span>}
      </p>
      {flat.length === 0 && <p className="scm__empty">No changes</p>}
      {sections.map((k) => (
        <section key={k} className="scm__group" aria-label={GROUP_LABEL[k]}>
          <h3 className="scm__heading">
            {GROUP_LABEL[k]} <span className="scm__count">{g[k].length}</span>
          </h3>
          <ul className="scm__list">
            {g[k].map((f) => {
              const i = ++index;
              const m = k === "staged" ? markOf({ ...f, worktree: "unmodified" }) : markOf({ ...f, index: k === "changes" ? "unmodified" : f.index });
              const name = f.path.split("/").pop() ?? f.path;
              const dir = f.path.slice(0, f.path.length - name.length).replace(/\/$/, "");
              return (
                <li key={`${k}:${f.path}`}>
                  <button
                    ref={(el) => {
                      refs.current[i] = el;
                    }}
                    type="button"
                    className="scm__row"
                    data-git={m?.kind}
                    data-path={f.path}
                    tabIndex={i === 0 ? 0 : -1}
                    aria-label={`${f.path}, ${m?.word ?? "changed"}${k === "staged" ? ", staged" : ""}`}
                    data-hint="Show the changes — Enter · Open the file — Shift+Enter"
                    onClick={(e) => (e.shiftKey ? onOpenFile(f.path) : git.openDiff(f.path))}
                    onKeyDown={(e) => onKey(e, i, f)}
                  >
                    <span className="scm__name">{name}</span>
                    {dir && <span className="scm__dir">{dir}</span>}
                    <span className="scm__letter" aria-hidden="true">
                      {m?.letter}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
