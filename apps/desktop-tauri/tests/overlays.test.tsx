// Overlay primitives (SYSTEM-004): the toast queue, the context-menu keyboard model,
// tooltips as names on focus, and roles. Markup from react-dom/server, no DOM.

import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { chordKeys, menuKey, TOAST_MAX, TOAST_MS, ToastQueue, ToastView } from "../src/ui/overlays";
import { IconButton } from "../src/ui/primitives";

describe("overlays", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("toast queue caps at 3", () => {
    const q = new ToastQueue();
    for (let i = 1; i <= 5; i++) q.push({ tone: "ok", text: `t${i}` });
    expect(TOAST_MAX).toBe(3);
    expect(q.snapshot().map((t) => t.text)).toEqual(["t3", "t4", "t5"]); // the oldest go first
    vi.advanceTimersByTime(TOAST_MS + 1);
    expect(q.snapshot()).toEqual([]);
  });

  it("toast pauses on hover", () => {
    const q = new ToastQueue();
    const id = q.push({ tone: "ok", text: "Saved" });
    vi.advanceTimersByTime(3000);
    q.pause(id); // pointer enters (or focus moves into) the toast
    vi.advanceTimersByTime(10_000);
    expect(q.snapshot()).toHaveLength(1);
    q.resume(id); // the remaining second runs again
    vi.advanceTimersByTime(900);
    expect(q.snapshot()).toHaveLength(1);
    vi.advanceTimersByTime(200);
    expect(q.snapshot()).toHaveLength(0);
  });

  it("toast does not steal focus", () => {
    const ok = renderToStaticMarkup(<ToastView item={{ id: 1, tone: "ok", text: "Saved executor.rs" }} />);
    const err = renderToStaticMarkup(<ToastView item={{ id: 2, tone: "err", text: "Save failed", action: { label: "Retry", run: () => {} } }} />);
    expect(ok).toContain('role="status"');
    expect(err).toContain('role="alert"');
    for (const html of [ok, err]) {
      expect(html).not.toMatch(/autofocus|tabindex="0"/i);
    }
    // the queue itself never touches focus: pushing works without any DOM at all
    expect(() => new ToastQueue().push({ tone: "err", text: "x" })).not.toThrow();
  });

  it("IconButton exposes name via tooltip on focus", () => {
    const html = renderToStaticMarkup(
      <IconButton label="Toggle panel" shortcut="Ctrl+J">
        <svg />
      </IconButton>,
    );
    // The name stays on the control itself (aria-label), the tooltip repeats it; no title.
    expect(html).toContain('aria-label="Toggle panel"');
    expect(html).not.toContain("title=");
    // Radix's trigger state: the tooltip opens on hover and on focus.
    expect(html).toContain('data-state="closed"');
    expect(chordKeys("Ctrl+J")).toBe("Ctrl J");
    expect(chordKeys("Shift+F5")).toBe("Shift F5");
    expect(chordKeys("…/src/axiom")).toBeNull(); // a hint, not keycaps
  });

  it("context menu arrow keys and Escape", () => {
    const disabled = [false, true, false];
    expect(menuKey("ArrowDown", 0, disabled)).toBe(2); // skips the disabled item
    expect(menuKey("ArrowDown", 2, disabled)).toBe(0); // wraps
    expect(menuKey("ArrowUp", 0, disabled)).toBe(2);
    expect(menuKey("Home", 2, disabled)).toBe(0);
    expect(menuKey("End", 0, disabled)).toBe(2);
    expect(menuKey("Escape", 0, disabled)).toBe("close");
    expect(menuKey("a", 0, disabled)).toBeNull();
  });
});
