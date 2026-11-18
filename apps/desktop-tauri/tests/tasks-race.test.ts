// A fast task's notifications can be dispatched before the store handles the `task/start`
// response that precedes them on the wire: the response resolves a promise whose continuation
// runs after the lines read in the same chunk. The store must not lose them (CI flake of
// "clear empties the output, rerunLast…", reproduced here deterministically).
import { describe, expect, it } from "vitest";
import type { DaemonClient } from "../src/daemon/client";
import { TasksStore } from "../src/state/tasks";
import type { WorkspaceStore } from "../src/state/workspace";

function scripted() {
  const listeners = new Map<string, ((v: unknown) => void)[]>();
  let answer: ((v: unknown) => void) | null = null;
  const client = {
    ready: true,
    request: (method: string) => {
      if (method === "task/start") return new Promise((resolve) => (answer = resolve));
      if (method === "task/list") return Promise.resolve({ tasks: [] });
      return Promise.resolve({});
    },
    onNotification: (method: string, fn: (v: unknown) => void) => {
      listeners.set(method, [...(listeners.get(method) ?? []), fn]);
      return () => {};
    },
    onState: () => () => {},
  } as unknown as DaemonClient;
  const emit = (method: string, v: unknown) => listeners.get(method)?.forEach((f) => f(v));
  return { client, emit, answer: (v: unknown) => answer?.(v) };
}

const workspace = { isOpen: true, getSnapshot: () => ({ id: "w1" }), on: () => () => {} } as unknown as WorkspaceStore;

describe("task notifications racing the start response", () => {
  it("output and exit that arrive before task/start is handled are applied, in order", async () => {
    const c = scripted();
    const tasks = new TasksStore(c.client, workspace);
    tasks.run("echo one");
    expect(tasks.getSnapshot().running).toBe(true);
    // The response is read first, but its continuation runs after these synchronous dispatches.
    c.answer({ taskId: "t1" });
    c.emit("task/output", { taskId: "t1", seq: 0, stream: "stdout", data: "one\n" });
    c.emit("task/exited", { taskId: "t1", outcome: "succeeded", exitCode: 0, durationMs: 3 });
    await new Promise((r) => setTimeout(r, 50));
    tasks.flushNow();
    const s = tasks.getSnapshot();
    expect(s.rows.map((r) => r.text)).toContain("one");
    expect(s.running).toBe(false);
    expect(s.lastOutcome).toBe("succeeded");
  });

  it("early notifications of another task are not applied", async () => {
    const c = scripted();
    const tasks = new TasksStore(c.client, workspace);
    tasks.run("echo two");
    c.emit("task/output", { taskId: "other", seq: 0, stream: "stdout", data: "stray\n" });
    c.emit("task/exited", { taskId: "other", outcome: "failed", exitCode: 1, durationMs: 1 });
    c.answer({ taskId: "t2" });
    await new Promise((r) => setTimeout(r, 50));
    tasks.flushNow();
    expect(tasks.getSnapshot().rows.map((r) => r.text)).not.toContain("stray");
    expect(tasks.getSnapshot().running).toBe(true);
  });
});
