// A line diff for the editor's change markers (M4-GIT-002): HEAD text against the buffer, as
// ranges of added, modified and deleted lines. Myers' O(ND) algorithm over lines, bounded (a diff
// with more than MAX_EDITS edits is reported as one modified block). Pure.

export interface LineChange {
  kind: "added" | "modified" | "deleted";
  /** 1-based first line in the buffer (for "deleted": the line after which lines went). */
  start: number;
  /** 1-based last line (inclusive); for "deleted", equal to start. */
  end: number;
}

const MAX_EDITS = 2000;

function lines(text: string): string[] {
  if (text === "") return [];
  const l = text.split("\n");
  if (l.length > 1 && l[l.length - 1] === "") l.pop();
  return l;
}

/** The edit script as runs of equal / delete / insert line counts (Myers, with a trace). */
function script(a: string[], b: string[]): { op: "eq" | "del" | "ins"; n: number }[] | null {
  const n = a.length;
  const m = b.length;
  const max = n + m;
  const v = new Map<number, number>([[1, 0]]);
  const trace: Map<number, number>[] = [];
  let found = false;
  for (let d = 0; d <= Math.min(max, MAX_EDITS); d++) {
    trace.push(new Map(v));
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && (v.get(k - 1) ?? 0) < (v.get(k + 1) ?? 0)) ? (v.get(k + 1) ?? 0) : (v.get(k - 1) ?? 0) + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x++;
        y++;
      }
      v.set(k, x);
      if (x >= n && y >= m) {
        found = true;
        break;
      }
    }
    if (found) break;
  }
  if (!found) return null;
  // Backtrack.
  const ops: ("eq" | "del" | "ins")[] = [];
  let x = n;
  let y = m;
  for (let d = trace.length - 1; d >= 0; d--) {
    const vd = trace[d]!;
    const k = x - y;
    const prevK = k === -d || (k !== d && (vd.get(k - 1) ?? 0) < (vd.get(k + 1) ?? 0)) ? k + 1 : k - 1;
    const prevX = vd.get(prevK) ?? 0;
    const prevY = prevX - prevK;
    while (x > prevX && y > prevY) {
      ops.push("eq");
      x--;
      y--;
    }
    if (d > 0) ops.push(x === prevX ? "ins" : "del");
    x = prevX;
    y = prevY;
  }
  ops.reverse();
  const runs: { op: "eq" | "del" | "ins"; n: number }[] = [];
  for (const op of ops) {
    const last = runs[runs.length - 1];
    if (last && last.op === op) last.n++;
    else runs.push({ op, n: 1 });
  }
  return runs;
}

/** The changed line ranges of `current` against `base`. */
export function lineChanges(base: string, current: string): LineChange[] {
  const a = lines(base);
  const b = lines(current);
  const runs = script(a, b);
  if (!runs) return b.length ? [{ kind: "modified", start: 1, end: b.length }] : [];
  const out: LineChange[] = [];
  let line = 1; // next line of `current`
  for (let i = 0; i < runs.length; i++) {
    const r = runs[i]!;
    if (r.op === "eq") {
      line += r.n;
      continue;
    }
    // A deletion followed by an insertion (or the reverse) is a modification of the inserted lines.
    const next = runs[i + 1];
    if ((r.op === "del" && next?.op === "ins") || (r.op === "ins" && next?.op === "del")) {
      const ins = r.op === "ins" ? r.n : next!.n;
      out.push({ kind: "modified", start: line, end: line + ins - 1 });
      line += ins;
      i++;
      continue;
    }
    if (r.op === "ins") {
      out.push({ kind: "added", start: line, end: line + r.n - 1 });
      line += r.n;
    } else {
      const at = Math.max(1, line - 1);
      out.push({ kind: "deleted", start: at, end: at });
    }
  }
  return out;
}
