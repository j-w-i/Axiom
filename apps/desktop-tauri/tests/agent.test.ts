// The Chat store (M3-UI-001): transcript from events (rewinds, tool results, interrupts), replay
// and live events deduplicated by seq, deltas coalesced per frame, several sessions at once,
// edit/regenerate through `fromTurn`, model choice.
import { describe, expect, it } from "vitest";
import type { DaemonClient } from "../src/daemon/client";
import type { AgentEvent, EventBody } from "../src/daemon/protocol.gen";
import { AgentStore, filterSessions, lastUser, providerOptions, transcript } from "../src/state/agent";
import type { WorkspaceStore } from "../src/state/workspace";

const ev = (seq: number, turn: number, event: EventBody, sessionId = "s1"): AgentEvent => ({ sessionId, seq, tsMs: seq, turn, event });
const user = (text: string): EventBody => ({ type: "user", text });
const said = (text: string): EventBody => ({ type: "assistantText", text });

function fake(opts: { open?: AgentEvent[] } = {}) {
  const listeners = new Map<string, ((v: unknown) => void)[]>();
  const calls: { method: string; params: Record<string, unknown> }[] = [];
  let n = 0;
  const client = {
    ready: true,
    request: async (method: string, params: Record<string, unknown> = {}) => {
      calls.push({ method, params });
      switch (method) {
        case "agent/models":
          return {
            providers: [
              { config: { id: "claude-code", kind: "acp", name: "Claude Code" }, hasKey: false, models: [], error: "Not installed." },
              {
                config: { id: "scripted", kind: "scripted", name: "Scripted" },
                hasKey: true,
                models: [{ id: "scripted", displayName: "Scripted", capabilities: { tools: "yes", streamingToolArgs: "yes", parallelToolCalls: "yes", images: "no" } }],
              },
            ],
          };
        case "agent/sessionList":
          return { sessions: [] };
        case "agent/sessionCreate":
          return { sessionId: `s${++n}`, title: "New conversation", providerId: "scripted", model: "scripted", createdMs: 1, updatedMs: 1, running: false, pendingApprovals: 0 };
        case "agent/sessionOpen":
          return {
            session: { sessionId: params.sessionId, title: "t", providerId: "scripted", model: "scripted", createdMs: 1, updatedMs: 1, running: false, pendingApprovals: 0 },
            events: opts.open ?? [],
          };
        default:
          return {};
      }
    },
    onNotification: (method: string, fn: (v: unknown) => void) => {
      listeners.set(method, [...(listeners.get(method) ?? []), fn]);
      return () => {};
    },
    onState: () => () => {},
  } as unknown as DaemonClient;
  const workspace = { getSnapshot: () => ({ id: "" }), subscribe: () => () => {} } as unknown as WorkspaceStore;
  const store = new AgentStore(client, workspace);
  store.storage = null;
  return { store, calls, emit: (m: string, v: unknown) => listeners.get(m)?.forEach((f) => f(v)) };
}

const frame = () => new Promise((r) => setTimeout(r, 30));

describe("transcript", () => {
  it("applies rewinds, tool results and interruptions", () => {
    const items = transcript([
      ev(1, 1, user("hi")),
      ev(2, 1, said("hello")),
      ev(3, 1, { type: "turnEnd", reason: "endTurn" }),
      ev(4, 2, user("read it")),
      ev(5, 2, { type: "toolCall", callId: "c1", tool: "fs.read", input: {}, effect: "read", targets: ["a.rs"] }),
      ev(6, 2, { type: "toolResult", callId: "c1", isError: false, summary: "lines 1–3 of 3", output: {} }),
      ev(7, 2, { type: "toolCall", callId: "c2", tool: "task.run", input: {}, effect: "execute" }),
      ev(8, 2, { type: "interrupted", callId: "c2" }),
      ev(9, 2, { type: "rewind", fromTurn: 2 }),
      ev(10, 2, user("read it again")),
    ]);
    expect(items.map((i) => i.kind)).toEqual(["user", "assistant", "user"]);
    expect(items[2]).toMatchObject({ text: "read it again", turn: 2 });
    const before = transcript([
      ev(4, 2, user("x")),
      ev(5, 2, { type: "toolCall", callId: "c1", tool: "fs.read", input: {}, effect: "read" }),
      ev(6, 2, { type: "toolResult", callId: "c1", isError: true, summary: "denied", output: {} }),
      ev(7, 2, { type: "toolCall", callId: "c2", tool: "task.run", input: {}, effect: "execute" }),
      ev(8, 2, { type: "interrupted", callId: "c2" }),
    ]);
    expect(before.filter((i) => i.kind === "tool").map((i) => (i.kind === "tool" ? `${i.state}:${i.summary}` : ""))).toEqual([
      "error:denied",
      "error:Interrupted: Axiom stopped while it ran",
    ]);
    expect(lastUser(items)?.text).toBe("read it again");
  });

  it("filters sessions and maps providers", () => {
    const s = (title: string) => ({ sessionId: title, title, providerId: "p", model: "m", createdMs: 0, updatedMs: 0, running: false, pendingApprovals: 0 });
    expect(filterSessions([s("Trip plan"), s("Rust help")], "rust").map((x) => x.title)).toEqual(["Rust help"]);
    const opts = providerOptions({
      providers: [{ config: { id: "a", kind: "anthropic", name: "Anthropic" }, hasKey: false, models: [], error: "No API key" }],
    });
    expect(opts[0]).toMatchObject({ providerId: "a", error: "No API key", models: [] });
  });
});

describe("AgentStore", () => {
  it("picks the first usable model, skipping unavailable providers and agents outside a folder", async () => {
    const { store } = fake();
    await store.load();
    expect(store.getSnapshot().model).toEqual({ providerId: "scripted", model: "scripted" });
    expect(store.getSnapshot().status).toBe("ready");
  });

  it("creates the conversation on the first send, then streams deltas per frame and dedupes", async () => {
    const { store, calls, emit } = fake();
    await store.load();
    expect(await store.send("hi")).toBe(true);
    const create = calls.find((c) => c.method === "agent/sessionCreate")!;
    expect(create.params).toMatchObject({ providerId: "scripted", model: "scripted" });
    expect(create.params.workspaceId).toBeUndefined();
    expect(calls.find((c) => c.method === "agent/send")!.params).toMatchObject({ sessionId: "s1", text: "hi" });
    expect(store.getSnapshot().active.running).toBe(true);

    emit("agent/event", ev(1, 1, user("hi")));
    emit("agent/event", ev(2, 1, { type: "textDelta", text: "Hel" }));
    emit("agent/event", ev(2, 1, { type: "textDelta", text: "lo" }));
    expect(store.getSnapshot().active.streaming).toBe(""); // not yet: one frame later
    await frame();
    expect(store.getSnapshot().active.streaming).toBe("Hello");
    emit("agent/event", ev(2, 1, said("Hello")));
    emit("agent/event", ev(2, 1, said("Hello"))); // a duplicate (replay overlap)
    expect(store.getSnapshot().active.streaming).toBe("");
    expect(store.items().filter((i) => i.kind === "assistant")).toHaveLength(1);
    emit("agent/event", ev(3, 1, { type: "turnEnd", reason: "endTurn" }));
    expect(store.getSnapshot().active.running).toBe(false);
  });

  it("keeps a background conversation updating while another is open", async () => {
    const { store, emit } = fake();
    await store.load();
    await store.send("first");
    emit("agent/event", ev(1, 1, user("first"), "s1"));
    store.newChat();
    await store.send("second");
    expect(store.getSnapshot().activeId).toBe("s2");
    emit("agent/event", ev(2, 1, said("answer one"), "s1"));
    expect(store.items()).toEqual([]);
    await store.open("s1");
    // The replay (empty in this fake) plus what arrived meanwhile.
    expect(store.items().map((i) => (i.kind === "assistant" ? i.text : i.kind))).toEqual(["user", "answer one"]);
  });

  it("edit and regenerate resend from the turn", async () => {
    const { store, calls, emit } = fake();
    await store.load();
    await store.send("what is 2+2?");
    emit("agent/event", ev(1, 1, user("what is 2+2?")));
    emit("agent/event", ev(2, 1, said("4")));
    emit("agent/event", ev(3, 1, { type: "turnEnd", reason: "endTurn" }));
    await store.regenerate();
    expect(calls.filter((c) => c.method === "agent/send").at(-1)!.params).toMatchObject({ text: "what is 2+2?", fromTurn: 1 });
    await store.edit(1, "what is 3+3?");
    expect(calls.filter((c) => c.method === "agent/send").at(-1)!.params).toMatchObject({ text: "what is 3+3?", fromTurn: 1 });
  });

  it("tracks approvals and drops them when resolved", () => {
    const { store, emit } = fake();
    const r = { requestId: "r1", sessionId: "s1", principal: "session:s1", tool: "fs.applyPatch", effect: "writeWorkspace", targets: ["a"], checkpointPlanned: true, canRemember: true };
    emit("agent/approvalRequested", r);
    expect(store.getSnapshot().approvals).toHaveLength(1);
    emit("agent/approvalResolved", { requestId: "r1", decision: "once" });
    expect(store.getSnapshot().approvals).toHaveLength(0);
  });
});
