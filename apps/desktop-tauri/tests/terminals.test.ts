// The Terminal view's keyboard rules and its store (M4-TERM-002).
import { describe, expect, it } from "vitest";
import type { DaemonClient } from "../src/daemon/client";
import { clipboardChord, isAppChord } from "../src/components/TerminalView";
import { TerminalsStore } from "../src/state/terminals";
import type { WorkspaceStore } from "../src/state/workspace";

const key = (key: string, m: Partial<{ ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }> = {}) => ({ key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...m });

describe("terminal keys", () => {
  it("the app's global chords pass the terminal by; everything else is the shell's", () => {
    for (const k of ["k", "p", "j", "b", "`", ",", "l"]) expect(isAppChord(key(k, { ctrlKey: true }), false), k).toBe(true);
    expect(isAppChord(key("c", { ctrlKey: true }), false)).toBe(false); // the interrupt
    expect(isAppChord(key("d", { ctrlKey: true }), false)).toBe(false); // end of input
    expect(isAppChord(key("r", { ctrlKey: true }), false)).toBe(false); // shell history search
    expect(isAppChord(key("k", { ctrlKey: true, altKey: true }), false)).toBe(false);
    expect(isAppChord(key("a"), false)).toBe(false);
    // macOS: Cmd chords are the app's, Ctrl chords the shell's.
    expect(isAppChord(key("k", { metaKey: true }), true)).toBe(true);
    expect(isAppChord(key("c", { ctrlKey: true }), true)).toBe(false);
  });

  it("copy and paste: Ctrl+Shift+C/V (Linux, Windows), Cmd+C/V (macOS); Ctrl+C is not copy", () => {
    expect(clipboardChord(key("C", { ctrlKey: true, shiftKey: true }), false)).toBe("copy");
    expect(clipboardChord(key("V", { ctrlKey: true, shiftKey: true }), false)).toBe("paste");
    expect(clipboardChord(key("c", { ctrlKey: true }), false)).toBeNull();
    expect(clipboardChord(key("c", { metaKey: true }), true)).toBe("copy");
    expect(clipboardChord(key("v", { metaKey: true }), true)).toBe("paste");
    expect(clipboardChord(key("c", { ctrlKey: true }), true)).toBeNull();
  });
});

function fake() {
  const listeners = new Map<string, ((v: unknown) => void)[]>();
  const calls: { method: string; params: Record<string, unknown> }[] = [];
  let n = 0;
  const client = {
    request: async (method: string, params: Record<string, unknown> = {}) => {
      calls.push({ method, params });
      if (method === "terminal/create") return { terminalId: `t${++n}`, workspaceId: "w1", title: "zsh", running: true };
      if (method === "terminal/list") return { terminals: [{ terminalId: "t9", workspaceId: "w1", title: "bash", running: true, backlog: "$ ls\r\n", seq: 4 }] };
      return {};
    },
    onNotification: (method: string, fn: (v: unknown) => void) => {
      listeners.set(method, [...(listeners.get(method) ?? []), fn]);
      return () => {};
    },
  };
  const handlers: Record<string, (() => void)[]> = { opened: [], closed: [] };
  const ws = { getSnapshot: () => ({ id: "w1", root: "/w" }), on: (e: string, fn: () => void) => void handlers[e]!.push(fn) };
  const emit = (m: string, p: unknown) => listeners.get(m)?.forEach((f) => f(p));
  return { store: new TerminalsStore(client as unknown as DaemonClient, ws as unknown as WorkspaceStore), calls, emit, fire: (e: string) => handlers[e]!.forEach((f) => f()) };
}

describe("the terminals store", () => {
  it("creates, keeps early output for the screen, labels duplicates, marks the exit, closes", async () => {
    const f = fake();
    const created = f.store.create(80, 24);
    f.emit("terminal/output", { terminalId: "t1", seq: 1, data: "$ " }); // before the create's answer
    expect(await created).toBe("t1");
    const got: string[] = [];
    f.store.onTerminalOutput("t1", (d) => got.push(d));
    f.emit("terminal/output", { terminalId: "t1", seq: 2, data: "hi" });
    f.emit("terminal/output", { terminalId: "t1", seq: 2, data: "dup" }); // replayed: ignored
    expect(got).toEqual(["$ ", "hi"]);
    await f.store.create(80, 24);
    const [a, b] = f.store.getSnapshot().terminals;
    expect([f.store.label(a!), f.store.label(b!)]).toEqual(["zsh 1", "zsh 2"]);
    expect(f.store.getSnapshot().active).toBe("t2");
    f.emit("terminal/exited", { terminalId: "t1", code: 0 });
    expect(f.store.label(f.store.getSnapshot().terminals[0]!)).toBe("zsh 1 (ended)");
    await f.store.close("t2");
    expect(f.store.getSnapshot().active).toBe("t1");
    expect(f.calls.map((c) => c.method)).toContain("terminal/close");
  });

  it("after a reload, reattaches with the backlog first and skips output already in it", async () => {
    const f = fake();
    f.fire("opened");
    await new Promise((r) => setTimeout(r, 0));
    expect(f.store.getSnapshot().active).toBe("t9");
    f.emit("terminal/output", { terminalId: "t9", seq: 4, data: "old" });
    const got: string[] = [];
    f.store.onTerminalOutput("t9", (d) => got.push(d));
    f.emit("terminal/output", { terminalId: "t9", seq: 5, data: "new" });
    expect(got).toEqual(["$ ls\r\n", "new"]);
  });
});
