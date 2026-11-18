// The settings store keeps a change that arrives while a load is in flight (requests are answered
// in any order, IPC 0.1): a stale settings/get answer must not overwrite it.

import { describe, expect, it } from "vitest";
import type { DaemonClient } from "../src/daemon/client";
import { SettingsStore } from "../src/state/settings";

function fakeClient() {
  const notify = new Map<string, (p: unknown) => void>();
  const state: ((s: { state: string }) => void)[] = [];
  let answer: ((v: unknown) => void) | null = null;
  const client = {
    onNotification: (method: string, fn: (p: unknown) => void) => void notify.set(method, fn),
    onState: (fn: (s: { state: string }) => void) => void state.push(fn),
    request: (method: string) => (method === "settings/get" ? new Promise((r) => (answer = r)) : Promise.resolve({})),
  } as unknown as DaemonClient;
  return {
    client,
    ready: () => state.forEach((fn) => fn({ state: "ready" })),
    didChange: (values: Record<string, unknown>) => notify.get("settings/didChange")!({ values }),
    answerGet: (values: Record<string, unknown>) => answer!({ values }),
  };
}

describe("settings store", () => {
  it("a change during a load survives the load's older answer", async () => {
    const f = fakeClient();
    const s = new SettingsStore(f.client);
    f.ready(); // load starts: settings/get in flight
    f.didChange({ "appearance.motion": "reduced" }); // a newer change arrives first
    f.answerGet({ "appearance.motion": "off", "appearance.theme": "dark" }); // the older answer
    await new Promise((r) => setTimeout(r, 0));
    expect(s.getSnapshot().motion).toBe("reduced"); // the change is kept
    expect(s.getSnapshot().theme).toBe("dark"); // other keys from the load still apply
    expect(s.getSnapshot().loaded).toBe(true);
  });
});
