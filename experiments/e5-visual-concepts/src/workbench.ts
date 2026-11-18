import { useCallback, useEffect, useRef, useState } from "react";
import { ACTIVE_FILE, DOCS } from "./data";

export type View = "workspace" | "environments" | "settings" | "components";

export interface Workbench {
  concept: "a" | "b" | "c" | "r";
  theme: "dark" | "light";
  view: View;
  setView: (v: View) => void;
  active: string;
  setActive: (p: string) => void;
  sidebar: boolean;
  panel: boolean;
  dock: boolean;
  toggle: (what: "sidebar" | "panel" | "dock") => void;
  palette: "commands" | "files" | null;
  setPalette: (p: "commands" | "files" | null) => void;
  toast: boolean;
  sideW: number;
  setSideW: (w: number) => void;
  dockW: number;
  panelH: number;
  setPanelH: (h: number) => void;
}

export function useWorkbench(): Workbench {
  const q = new URLSearchParams(location.search);
  const concept = (q.get("c") ?? "a") as Workbench["concept"];
  const theme = (q.get("theme") ?? "dark") as Workbench["theme"];
  const [view, setView] = useState<View>((q.get("view") as View) ?? "workspace");
  const [active, setActive] = useState(q.get("doc") ?? ACTIVE_FILE);
  const [sidebar, setSidebar] = useState(q.get("sidebar") !== "0");
  const [panel, setPanel] = useState(q.get("panel") !== "0");
  const [dock, setDock] = useState(q.get("dock") !== "0");
  const [palette, setPalette] = useState<Workbench["palette"]>((q.get("palette") as Workbench["palette"]) ?? null);
  const [toast, setToast] = useState(q.has("toast"));
  const [sideW, setSideW] = useState(concept === "c" ? 248 : 252);
  const [panelH, setPanelH] = useState(232);
  const toggle = useCallback((w: "sidebar" | "panel" | "dock") => {
    if (w === "sidebar") setSidebar((v) => !v);
    if (w === "panel") setPanel((v) => !v);
    if (w === "dock") setDock((v) => !v);
  }, []);
  const toastTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const k = e.key.toLowerCase();
      let hit = true;
      if (k === "k") setPalette("commands");
      else if (k === "p") setPalette("files");
      else if (k === "b") toggle("sidebar");
      else if (k === "j") toggle("panel");
      else if (k === "l") toggle("dock");
      else if (k === ",") setView("settings");
      else if (k === "s") {
        setToast(true);
        clearTimeout(toastTimer.current);
        toastTimer.current = setTimeout(() => setToast(false), 2200);
      } else if (k === "tab") {
        const i = DOCS.findIndex((d) => d.path === active);
        setActive(DOCS[(i + (e.shiftKey ? DOCS.length - 1 : 1)) % DOCS.length]!.path);
      } else hit = false;
      if (hit) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [active, toggle]);

  return {
    concept, theme, view, setView, active, setActive, sidebar, panel, dock, toggle, palette, setPalette, toast,
    sideW, setSideW, dockW: concept === "b" ? 344 : 352, panelH, setPanelH,
  };
}

/** Pointer-drag resizing. Writes the size through a CSS variable on `target`
 * during the drag (no React render per pointer event), commits on release. */
export function useResize(
  target: React.RefObject<HTMLElement | null>,
  varName: string,
  axis: "x" | "y",
  invert: boolean,
  value: number,
  commit: (v: number) => void,
  min: number,
  max: number,
) {
  return (e: React.PointerEvent) => {
    const start = axis === "x" ? e.clientX : e.clientY;
    const el = target.current;
    if (!el) return;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    handle.dataset.dragging = "true";
    let cur = value;
    let raf = 0;
    const move = (ev: PointerEvent) => {
      const d = ((axis === "x" ? ev.clientX : ev.clientY) - start) * (invert ? -1 : 1);
      cur = Math.max(min, Math.min(max, value + d));
      if (!raf) raf = requestAnimationFrame(() => { raf = 0; el.style.setProperty(varName, `${cur}px`); });
    };
    const up = () => {
      handle.dataset.dragging = "false";
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      commit(cur);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  };
}
