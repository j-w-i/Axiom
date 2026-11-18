// The console's rows and prompt (SHELL-006; workspace-spec.md §2.4, D6): stderr is marked by a
// gutter glyph and a hidden "stderr:" prefix, not painted; the prompt follows `running`.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { OutputPanel } from "../src/components/OutputPanel";
import type { ProblemsStore } from "../src/state/problems";
import type { TasksStore } from "../src/state/tasks";
import type { WorkspaceStore } from "../src/state/workspace";

function render(state: Record<string, unknown>) {
  const snap = { running: false, label: "", statusText: "", lastOutcome: null, history: [], rows: [], dropped: 0, startedAt: null, outputSeq: 0, ...state };
  const tasks = { subscribe: () => () => {}, getSnapshot: () => snap, allText: () => "", lastLabel: (snap.history as string[])[0] ?? "" } as unknown as TasksStore;
  const problems = { subscribe: () => () => {}, getSnapshot: () => ({ runs: {}, byTask: {} }), all: () => [] } as unknown as ProblemsStore;
  const ws = { id: "w1", name: "axiom", root: "/w", recent: [] };
  const workspace = { subscribe: () => () => {}, getSnapshot: () => ws } as unknown as WorkspaceStore;
  return renderToStaticMarkup(<OutputPanel tasks={tasks} workspace={workspace} problems={problems} ready onHide={() => {}} onOpenProblem={() => {}} />);
}

const rows = [
  { id: 1, text: "   Compiling axiom v0.1.0", stream: "stderr", kind: "line" },
  { id: 2, text: "running 3 tests", stream: "stdout", kind: "line" },
  { id: 3, text: "✖ Failed with exit code 101 in 4.8 s", stream: "", kind: "statusFailed" },
];

describe("console", () => {
  it("stderr row has gutter and hidden prefix", () => {
    const html = render({ rows, lastOutcome: "failed", history: ["cargo build"] });
    const row = (id: number) => html.split('class="output__row"').find((r, i) => i > 0 && r.includes(rows[id - 1]!.text.trim())) ?? "";
    expect(row(1)).toContain('data-stream="stderr"');
    expect(row(1)).toMatch(/class="output__gutter" aria-hidden="true">▏<\/span>/);
    expect(row(1)).toContain('<span class="visually-hidden">stderr: </span>');
    expect(row(2)).toMatch(/class="output__gutter" aria-hidden="true"><\/span>/);
    expect(row(2)).not.toContain("stderr: ");
    expect(row(3)).toContain('data-kind="statusFailed"');
  });

  it("stderr is not painted with --danger; only failure status and errors are (D6)", () => {
    const css = readFileSync(join(import.meta.dirname, "../src/styles/components/console.css"), "utf8");
    const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].map((m) => ({ sel: m[1]!.trim(), body: m[2]! }));
    const danger = rules.filter((r) => r.body.includes("--danger")).map((r) => r.sel);
    expect(danger).toEqual(['.output__row[data-kind="statusFailed"],\n.output__row[data-kind="error"]']);
    expect(rules.find((r) => r.sel === '.output__row[data-stream="stderr"]')?.body).toContain("--text-muted");
  });

  it("the prompt shows Run while idle and Stop while running; the header's view strip has Output and Problems", () => {
    const idle = render({});
    expect(idle).toContain('data-testid="run"');
    expect(idle).not.toContain('data-testid="stop"');
    expect(idle).toContain("Run a command in this workspace");
    // M2-UI-003: two views, so the strip exists (spec §2.4: it was hidden while Output was the only one).
    expect(idle).toContain('role="tablist" aria-label="Console views"');
    expect(idle.match(/role="tab"/g)).toHaveLength(2);
    expect(idle).toMatch(/aria-selected="true"[^>]*data-view="output"/);
    const running = render({ running: true, label: "cargo test", startedAt: Date.now() - 12_000 });
    expect(running).toContain('data-testid="stop"');
    expect(running).toContain('class="btn btn--sm btn--danger-quiet"');
    expect(running).toMatch(/class="console__label"[^>]*>cargo test</);
    expect(running).toMatch(/class="console__time">00:1[23]</);
  });
});
