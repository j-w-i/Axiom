// Workbench layout state (workspace-spec.md §1; M1.6-SHELL-002): side bar width and
// visibility, console height and visibility. Shell-local, per machine, in localStorage
// `axiom.layout.v2` (migrated once from v1). Pure functions, unit-tested.

export interface Layout {
  sidebar: number;
  panel: number;
  sidebarVisible: boolean;
  panelVisible: boolean;
}

export const LAYOUT_KEY = "axiom.layout.v2";
export const LAYOUT_V1_KEY = "axiom.layout.v1";

/** Size limits and defaults, px. */
export const LIMITS = {
  sidebar: { min: 180, max: 480, def: 252 },
  panel: { min: 120, max: 520, def: 232 },
} as const;

/** Below this window width the side bar starts hidden. */
export const NARROW = 900;

export interface WindowSize {
  width: number;
  height: number;
}

export function defaultLayout(win: WindowSize): Layout {
  return { sidebar: LIMITS.sidebar.def, panel: LIMITS.panel.def, sidebarVisible: win.width >= NARROW, panelVisible: true };
}

/** The largest side bar / console the window allows (the editor keeps room to work). */
export function maxSizes(win: WindowSize): { sidebar: number; panel: number } {
  return {
    sidebar: Math.max(LIMITS.sidebar.min, Math.min(LIMITS.sidebar.max, win.width - 400)),
    panel: Math.max(LIMITS.panel.min, Math.min(LIMITS.panel.max, win.height - 260)),
  };
}

export const clamp = (v: number, min: number, max: number): number => Math.max(min, Math.min(max, Math.round(v)));

/** Sizes within their limits and the window. Visibility is kept. */
export function clampLayout(l: Layout, win: WindowSize): Layout {
  const max = maxSizes(win);
  return { ...l, sidebar: clamp(l.sidebar, LIMITS.sidebar.min, max.sidebar), panel: clamp(l.panel, LIMITS.panel.min, max.panel) };
}

type Store = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function parse(text: string | null): Partial<Layout> | null {
  if (!text) return null;
  try {
    const v = JSON.parse(text) as unknown;
    if (!v || typeof v !== "object") return null;
    const o = v as Record<string, unknown>;
    const out: Partial<Layout> = {};
    if (typeof o.sidebar === "number") out.sidebar = o.sidebar;
    if (typeof o.panel === "number") out.panel = o.panel;
    if (typeof o.sidebarVisible === "boolean") out.sidebarVisible = o.sidebarVisible;
    if (typeof o.panelVisible === "boolean") out.panelVisible = o.panelVisible;
    return out;
  } catch {
    return null;
  }
}

/**
 * The stored layout: v2 if present; otherwise v1 is migrated (same keys and values),
 * written as v2 and removed; otherwise the defaults. Stored values are kept as they
 * are; the caller clamps them to the window (clampLayout).
 */
export function loadLayout(store: Store, win: WindowSize): Layout {
  const d = defaultLayout(win);
  const v2 = parse(store.getItem(LAYOUT_KEY));
  if (v2) return { ...d, ...v2 };
  const v1 = parse(store.getItem(LAYOUT_V1_KEY));
  if (v1) {
    const migrated = { ...d, ...v1 };
    store.setItem(LAYOUT_KEY, JSON.stringify(migrated));
    store.removeItem(LAYOUT_V1_KEY);
    return migrated;
  }
  return d;
}

export function saveLayout(store: Store, l: Layout): void {
  store.setItem(LAYOUT_KEY, JSON.stringify(l));
}

/**
 * The layout as a small external store: the workbench subscribes to all of it, the rest
 * of the shell only to the visibility flags, so a resize commit re-renders the workbench
 * alone (one React commit per drag).
 */
export class LayoutStore {
  private value: Layout;
  private listeners = new Set<() => void>();

  constructor(private readonly store: Store, win: WindowSize) {
    this.value = loadLayout(store, win);
  }

  subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };
  get = (): Layout => this.value;

  update(fn: (l: Layout) => Layout): void {
    const next = fn(this.value);
    if (next === this.value) return;
    this.value = next;
    saveLayout(this.store, next);
    for (const l of this.listeners) l();
  }
}
