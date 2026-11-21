// Workbench layout state (SHELL-002): v1 → v2 migration, defaults, clamping, and the
// splitter's keyboard rules.

import { describe, expect, it } from "vitest";
import { clampLayout, defaultLayout, LAYOUT_KEY, LAYOUT_V1_KEY, LIMITS, loadLayout, saveLayout } from "../src/shell/layout";
import { splitterKey } from "../src/ui/panel";

function memoryStore(init: Record<string, string> = {}) {
  const m = new Map(Object.entries(init));
  return {
    map: m,
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  };
}

const WIDE = { width: 1440, height: 900 };
const NARROW = { width: 800, height: 600 };

describe("layout", () => {
  it("migrates layout v1 to v2", () => {
    const v1 = { sidebar: 300, panel: 180, sidebarVisible: false, panelVisible: true };
    const store = memoryStore({ [LAYOUT_V1_KEY]: JSON.stringify(v1) });
    const l = loadLayout(store, WIDE);
    const migrated = { ...v1, dock: 352, dockVisible: false }; // identical values; the dock (M3) defaulted
    expect(l).toEqual(migrated);
    expect(store.map.has(LAYOUT_V1_KEY)).toBe(false); // read once, then removed
    expect(JSON.parse(store.map.get(LAYOUT_KEY)!)).toEqual(migrated);
    // v2 wins over a stale v1 from then on
    store.setItem(LAYOUT_V1_KEY, JSON.stringify({ sidebar: 999 }));
    expect(loadLayout(store, WIDE).sidebar).toBe(300);
    // nothing stored: defaults; garbage: defaults
    expect(loadLayout(memoryStore(), WIDE)).toEqual(defaultLayout(WIDE));
    expect(loadLayout(memoryStore({ [LAYOUT_KEY]: "{not json" }), WIDE)).toEqual(defaultLayout(WIDE));
  });

  it("reset restores defaults (narrow and wide window)", () => {
    // The dock (M3-UI-002) starts hidden at 352 px.
    expect(defaultLayout(WIDE)).toEqual({ sidebar: 252, panel: 232, sidebarVisible: true, panelVisible: true, dock: 352, dockVisible: false });
    expect(defaultLayout(NARROW)).toEqual({ sidebar: 252, panel: 232, sidebarVisible: false, panelVisible: true, dock: 352, dockVisible: false });
    const store = memoryStore();
    saveLayout(store, { sidebar: 400, panel: 400, sidebarVisible: false, panelVisible: false, dock: 500, dockVisible: true });
    saveLayout(store, defaultLayout(WIDE)); // what "Reset Layout" stores
    expect(loadLayout(store, WIDE)).toEqual(defaultLayout(WIDE));
  });

  it("clamps sizes to limits", () => {
    const l = { sidebar: 10, panel: 9999, sidebarVisible: true, panelVisible: true, dock: 352, dockVisible: false };
    expect(clampLayout(l, WIDE)).toEqual({ ...l, sidebar: LIMITS.sidebar.min, panel: LIMITS.panel.max });
    expect(clampLayout({ ...l, sidebar: 9999 }, WIDE).sidebar).toBe(LIMITS.sidebar.max);
    // the window bounds them too: 720x480 leaves room for the editor
    const tiny = clampLayout({ ...l, sidebar: 9999, panel: 9999 }, { width: 720, height: 480 });
    expect(tiny.sidebar).toBe(320);
    expect(tiny.panel).toBe(220);
    expect(clampLayout({ ...l, dock: 9999 }, WIDE).dock).toBe(LIMITS.dock.max);
    expect(clampLayout({ ...l, dock: 1 }, WIDE).dock).toBe(LIMITS.dock.min);
  });

  it("splitter keys: arrows ±16, Shift ±48, Home = default, inverted for the console", () => {
    const side = { orientation: "vertical" as const, min: 180, max: 480, defaultValue: 252 };
    expect(splitterKey("ArrowRight", false, 252, side)).toBe(268);
    expect(splitterKey("ArrowLeft", true, 252, side)).toBe(204);
    expect(splitterKey("Home", false, 400, side)).toBe(252);
    expect(splitterKey("ArrowLeft", true, 190, side)).toBe(180);
    expect(splitterKey("a", false, 252, side)).toBeNull();
    const consoleG = { orientation: "horizontal" as const, invert: true, min: 120, max: 520, defaultValue: 232 };
    expect(splitterKey("ArrowUp", false, 232, consoleG)).toBe(248); // seam up = console taller
    expect(splitterKey("ArrowDown", false, 232, consoleG)).toBe(216);
  });
});
