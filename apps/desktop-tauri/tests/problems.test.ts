// The Problems view's store and helpers, and provider tasks in the tasks store (M2-UI-003).
import { describe, expect, it } from "vitest";
import { problemTarget } from "../src/components/Problems";
import { viewTarget } from "../src/components/OutputPanel";
import type { DaemonClient } from "../src/daemon/client";
import type { Diagnostic } from "../src/daemon/protocol.gen";
import { counts, groupProblems, positionText, ProblemsStore } from "../src/state/problems";
import { providerTaskTitle, TasksStore } from "../src/state/tasks";
import type { WorkspaceStore } from "../src/state/workspace";

const d = (severity: Diagnostic["severity"], file: string | null, line = 1, column = 1, message = "m"): Diagnostic => ({
  severity,
  message,
  source: "rustc",
  related: [],
  ...(file ? { location: { file, line, column } } : {}),
});

function fakeClient() {
  const listeners = new Map<string, ((v: unknown) => void)[]>();
  const calls: { method: string; params: Record<string, unknown> }[] = [];
  const client = {
    ready: true,
    request: async (method: string, params: Record<string, unknown> = {}) => {
      calls.push({ method, params });
      if (method === "build/run") return { taskId: "t-build" };
      if (method === "task/start") return { taskId: "t-cmd" };
      if (method === "build/tasks") return { tasks: [{ providerId: "rust", id: "build", kind: "build", label: "cargo build" }] };
      return { tasks: [], values: {} };
    },
    onNotification: (method: string, fn: (v: unknown) => void) => {
      listeners.set(method, [...(listeners.get(method) ?? []), fn]);
      return () => {};
    },
    onState: () => () => {},
  } as unknown as DaemonClient;
  return { client, calls, emit: (m: string, v: unknown) => listeners.get(m)?.forEach((f) => f(v)) };
}

const workspace = { isOpen: true, getSnapshot: () => ({ id: "w1" }), on: () => () => {} } as unknown as WorkspaceStore;

describe("problems helpers", () => {
  it("groups by file (no-location last), severity first, then position", () => {
    const g = groupProblems([d("warning", "src/b.rs", 3), d("error", "src/b.rs", 9), d("error", "src/a.rs", 2, 5), d("info", null), d("error", "src/b.rs", 1)]);
    expect(g.map((x) => x.file)).toEqual(["src/a.rs", "src/b.rs", ""]);
    expect(g[1]!.rows.map((r) => `${r.diagnostic.severity}:${r.diagnostic.location?.line}`)).toEqual(["error:1", "error:9", "warning:3"]);
    expect(counts([d("error", null), d("warning", null), d("hint", null)])).toEqual({ errors: 1, warnings: 1, total: 3 });
    expect(positionText(d("error", "src/main.rs", 2, 18))).toBe("src/main.rs:2:18");
    expect(positionText(d("error", null))).toBe("");
  });

  it("keys", () => {
    expect(problemTarget("ArrowDown", 2, 3)).toBe(2);
    expect(problemTarget("ArrowUp", 0, 3)).toBe(0);
    expect(problemTarget("End", 0, 3)).toBe(2);
    expect(viewTarget("ArrowRight")).toBe("problems");
    expect(viewTarget("Home")).toBe("output");
    expect(viewTarget("x")).toBeNull();
    // M4-TERM-002: three views, wrapping; only the shown ones.
    expect(viewTarget("ArrowRight", "problems")).toBe("terminal");
    expect(viewTarget("ArrowRight", "terminal")).toBe("output");
    expect(viewTarget("ArrowLeft", "output")).toBe("terminal");
    expect(viewTarget("End")).toBe("terminal");
    expect(viewTarget("ArrowRight", "problems", ["output", "problems"])).toBe("output");
    expect(viewTarget("End", "output", ["output", "problems"])).toBe("problems");
  });
});

describe("problems store", () => {
  it("collects batches per task (even before the run is known) and a new run of the task replaces the old one", () => {
    const c = fakeClient();
    const p = new ProblemsStore(c.client, workspace);
    // Diagnostics that arrive before the start's response is handled are kept.
    c.emit("build/diagnostics", { taskId: "t1", diagnostics: [d("error", "src/a.rs")], complete: false });
    p.begin("rust/build", "t1", "Build: cargo build");
    c.emit("build/diagnostics", { taskId: "t1", diagnostics: [d("warning", "src/a.rs")], complete: true });
    expect(p.all()).toHaveLength(2);
    expect(p.getSnapshot().byTask.t1?.complete).toBe(true);
    // Another task's problems sit beside these.
    p.begin("rust/test", "t2", "Test: cargo test");
    c.emit("build/diagnostics", { taskId: "t2", diagnostics: [d("error", "tests/x.rs")], complete: true });
    expect(p.all()).toHaveLength(3);
    // A new run of rust/build clears its previous problems only.
    p.begin("rust/build", "t3", "Build: cargo build");
    expect(p.all().map((x) => x.location?.file)).toEqual(["tests/x.rs"]);
  });
});

describe("provider tasks in the tasks store", () => {
  it("lists them on open, runs one through build/run, tells the Problems store, and Rerun repeats it", async () => {
    const c = fakeClient();
    const tasks = new TasksStore(c.client, workspace);
    await tasks.loadProviderTasks();
    const t = tasks.getSnapshot().providerTasks[0]!;
    expect(providerTaskTitle(t)).toBe("Build: cargo build");
    const began: string[] = [];
    tasks.onProviderStart = (key, taskId, label) => began.push(`${key} ${taskId} ${label}`);
    tasks.runProvider(t);
    await new Promise((r) => setTimeout(r, 10));
    expect(c.calls.find((x) => x.method === "build/run")?.params).toEqual({ workspaceId: "w1", providerId: "rust", task: "build" });
    expect(began).toEqual(["rust/build t-build Build: cargo build"]);
    expect(tasks.getSnapshot().label).toBe("cargo build");
    expect(tasks.lastLabel).toBe("Build: cargo build");
    expect(tasks.getSnapshot().history).toEqual([]); // provider tasks are not command history
    c.emit("task/exited", { taskId: "t-build", outcome: "failed", exitCode: 101, durationMs: 5 });
    tasks.rerunLast();
    await new Promise((r) => setTimeout(r, 10));
    expect(c.calls.filter((x) => x.method === "build/run")).toHaveLength(2);
    // A command run makes the command the Rerun target again.
    c.emit("task/exited", { taskId: "t-build", outcome: "failed", exitCode: 101, durationMs: 5 });
    tasks.run("echo hi");
    expect(tasks.getSnapshot().lastProvider).toBeNull();
    expect(tasks.lastLabel).toBe("echo hi");
  });
});
