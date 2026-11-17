// Motion foundation (INTERACTION-001): level resolution, the appearance.motion
// setting, presence timing and the loop gate. No DOM: presence is tested through its
// state machine, the loop gate through a fake window.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DaemonClient } from "../src/daemon/client";
import { SettingsStore } from "../src/state/settings";
import { Presence, resolveMotion, startLoopGate, type PresenceState } from "../src/ui/motion";

function fakeClient(values: Record<string, unknown>) {
  const notify = new Map<string, (p: unknown) => void>();
  const client = {
    onNotification: (m: string, fn: (p: unknown) => void) => notify.set(m, fn),
    onState: () => () => {},
    request: async () => ({ values }),
  };
  return { client: client as unknown as DaemonClient, notify };
}

describe("motion", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("resolveMotion OS preference forces reduced", () => {
    expect(resolveMotion("standard", true)).toBe("reduced");
    expect(resolveMotion("reduced", true)).toBe("reduced");
    expect(resolveMotion("standard", false)).toBe("standard");
  });

  it("resolveMotion off stays off", () => {
    expect(resolveMotion("off", true)).toBe("off");
    expect(resolveMotion("off", false)).toBe("off");
  });

  it("settings maps appearance.motion with standard default", async () => {
    const absent = new SettingsStore(fakeClient({ "appearance.theme": "dark" }).client);
    await absent.load();
    expect(absent.getSnapshot().motion).toBe("standard");
    const off = new SettingsStore(fakeClient({ "appearance.motion": "off" }).client);
    await off.load();
    expect(off.getSnapshot().motion).toBe("off");
    const odd = new SettingsStore(fakeClient({ "appearance.motion": "wobbly" }).client);
    await odd.load();
    expect(odd.getSnapshot().motion).toBe("standard");
    // a later change arrives as settings/didChange
    const { client, notify } = fakeClient({});
    const live = new SettingsStore(client);
    notify.get("settings/didChange")?.({ values: { "appearance.motion": "reduced" } });
    expect(live.getSnapshot().motion).toBe("reduced");
  });

  it("usePresence keeps mounted for exitMs then unmounts", () => {
    const seen: PresenceState[] = [];
    const p = new Presence(true, (s) => seen.push(s), () => "standard");
    p.set(false, 140);
    expect(p.state).toEqual({ mounted: true, phase: "out" });
    vi.advanceTimersByTime(139);
    expect(p.state.mounted).toBe(true);
    vi.advanceTimersByTime(1);
    expect(p.state).toEqual({ mounted: false, phase: "out" });
    // reduced or off: no exit time
    const q = new Presence(true, () => {}, () => "off");
    q.set(false, 140);
    expect(q.state.mounted).toBe(false);
  });

  it("usePresence reopen during exit", () => {
    const p = new Presence(true, () => {}, () => "standard");
    p.set(false, 140);
    vi.advanceTimersByTime(60);
    p.set(true, 140);
    expect(p.state).toEqual({ mounted: true, phase: "in" });
    vi.advanceTimersByTime(500); // the cancelled exit never unmounts it
    expect(p.state).toEqual({ mounted: true, phase: "in" });
  });

  it("the loop gate marks the window idle on blur and hidden, and awake on focus", () => {
    const win = new EventTarget() as EventTarget & { document: EventTarget & { documentElement: { dataset: Record<string, string> }; visibilityState: string; hasFocus: () => boolean } };
    const doc = Object.assign(new EventTarget(), { documentElement: { dataset: {} as Record<string, string> }, visibilityState: "visible", hasFocus: () => true });
    win.document = doc;
    const stop = startLoopGate(win as unknown as Window);
    expect(doc.documentElement.dataset.idle).toBeUndefined();
    win.dispatchEvent(new Event("blur"));
    expect(doc.documentElement.dataset.idle).toBe("true");
    win.dispatchEvent(new Event("focus"));
    expect(doc.documentElement.dataset.idle).toBeUndefined();
    doc.visibilityState = "hidden";
    doc.dispatchEvent(new Event("visibilitychange"));
    expect(doc.documentElement.dataset.idle).toBe("true");
    stop();
    expect(doc.documentElement.dataset.idle).toBeUndefined();
  });
});
