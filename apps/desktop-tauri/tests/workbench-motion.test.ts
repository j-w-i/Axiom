// Region motion (INTERACTION-002): presence of the side bar / console through their exit, and
// the splitter's one-commit drag. No DOM: Presence is a plain class, and the splitter's
// pointer handler is driven with stand-in elements.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { REGION_EXIT_MS } from "../src/shell/Workbench";
import { Presence, type MotionLevel, type PresenceState } from "../src/ui/motion";
import { Splitter } from "../src/ui/panel";

function presence(open: boolean, level: MotionLevel = "standard") {
  const seen: PresenceState[] = [];
  const p = new Presence(open, (s) => seen.push(s), () => level);
  return { p, seen };
}

describe("workbench motion", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("side bar unmounts after exit duration", () => {
    const { p } = presence(true);
    p.set(false, REGION_EXIT_MS);
    expect(p.state).toEqual({ mounted: true, phase: "out" }); // exiting: still in the layout
    vi.advanceTimersByTime(REGION_EXIT_MS - 1);
    expect(p.state.mounted).toBe(true);
    vi.advanceTimersByTime(1);
    expect(p.state).toEqual({ mounted: false, phase: "out" }); // one change removes the column
    // Reduced and Off: no exit, removed at once.
    for (const level of ["reduced", "off"] as const) {
      const r = presence(true, level);
      r.p.set(false, REGION_EXIT_MS);
      expect(r.p.state.mounted).toBe(false);
    }
  });

  it("reopen during exit keeps mounted", () => {
    const { p, seen } = presence(true);
    // Five toggles in 300 ms ending open, then five ending closed.
    for (let i = 0; i < 5; i++) {
      p.set(i % 2 === 1, REGION_EXIT_MS);
      vi.advanceTimersByTime(60);
    }
    p.set(true, REGION_EXIT_MS);
    vi.advanceTimersByTime(500);
    expect(p.state).toEqual({ mounted: true, phase: "in" });
    expect(seen.every((s) => s.mounted || s.phase === "out")).toBe(true);
    for (let i = 0; i < 5; i++) {
      p.set(i % 2 === 0 ? false : true, REGION_EXIT_MS);
      vi.advanceTimersByTime(30);
    }
    p.set(false, REGION_EXIT_MS);
    vi.advanceTimersByTime(500);
    expect(p.state).toEqual({ mounted: false, phase: "out" }); // no stale phase, nothing half-visible
  });

  it("drag commits once", () => {
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => setTimeout(() => cb(0), 16) as unknown as number);
    vi.stubGlobal("cancelAnimationFrame", (id: number) => clearTimeout(id));
    const writes: string[] = [];
    const target = { dataset: {} as Record<string, string>, style: { setProperty: (_: string, v: string) => writes.push(v) } };
    const listeners: Record<string, (e: { clientX: number; clientY: number }) => void> = {};
    const handle = {
      dataset: {} as Record<string, string>,
      setPointerCapture: () => {},
      addEventListener: (t: string, f: (e: { clientX: number; clientY: number }) => void) => (listeners[t] = f),
      removeEventListener: (t: string) => delete listeners[t],
    };
    const commits: number[] = [];
    const el = Splitter({
      orientation: "vertical",
      label: "Resize side bar",
      value: 252,
      min: 180,
      max: 480,
      defaultValue: 252,
      target: { current: target as unknown as HTMLElement },
      cssVar: "--side-w",
      onCommit: (v) => commits.push(v),
    });
    el.props.onPointerDown({ button: 0, clientX: 100, clientY: 0, pointerId: 1, preventDefault: () => {}, currentTarget: handle });
    for (let x = 101; x <= 140; x++) {
      listeners.pointermove!({ clientX: x, clientY: 0 });
      if (x % 4 === 0) vi.advanceTimersByTime(16);
    }
    expect(commits).toEqual([]); // nothing committed while dragging
    listeners.pointerup!({ clientX: 140, clientY: 0 });
    expect(commits).toEqual([292]); // one commit, on pointer-up
    expect(writes.length).toBeLessThan(40); // CSS writes coalesced per frame, not per move
    expect(writes.at(-1)).toBe("292px");
    vi.unstubAllGlobals();
  });
});
