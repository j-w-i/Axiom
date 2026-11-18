// Region frame and splitter (design-system.md §9 Panel, Splitter; workspace-spec.md §1;
// M1.6-SHELL-002). Flat material: panels are separated by tone and 1 px seams, no
// gradients or lit edges. The splitter resizes without a React render per pointer event:
// it writes a CSS variable on its target once per animation frame and commits once,
// on pointer-up (motion-system.md §3.5: nothing animates during a drag).

import type { KeyboardEvent, PointerEvent, ReactNode, RefObject } from "react";
import "../styles/components/panel.css";

/** A region: <section aria-label=title data-region> with a 30 px header (title, meta, tools). */
export function Panel({
  title,
  meta,
  tools,
  region,
  className,
  testId,
  phase,
  children,
}: {
  title: string;
  meta?: ReactNode;
  tools?: ReactNode;
  region?: string;
  className?: string;
  testId?: string;
  /** Presence phase (ui/motion.ts) for a region that animates in and out. */
  phase?: "in" | "out";
  children: ReactNode;
}) {
  return (
    <section className={className ? `panel ${className}` : "panel"} aria-label={title} data-region={region} data-testid={testId} data-phase={phase}>
      <header className="panel__head">
        <span className="panel__title">{title}</span>
        {meta && <span className="panel__meta">{meta}</span>}
        {tools && <span className="panel__tools">{tools}</span>}
      </header>
      <div className="panel__body">{children}</div>
    </section>
  );
}

export interface SplitterGeometry {
  orientation: "vertical" | "horizontal";
  /** True when moving the seam right/down makes the controlled pane smaller (the console). */
  invert?: boolean;
  min: number;
  max: number;
  defaultValue: number;
}

/** The size a key produces (arrows ±16, Shift ±48, Home = default), or null when the key is not handled. */
export function splitterKey(key: string, shift: boolean, value: number, g: SplitterGeometry): number | null {
  const step = shift ? 48 : 16;
  const dec = g.orientation === "vertical" ? "ArrowLeft" : "ArrowUp";
  const inc = g.orientation === "vertical" ? "ArrowRight" : "ArrowDown";
  const clampTo = (v: number) => Math.max(g.min, Math.min(g.max, v));
  if (key === "Home") return clampTo(g.defaultValue);
  if (key === dec) return clampTo(value + (g.invert ? step : -step));
  if (key === inc) return clampTo(value + (g.invert ? -step : step));
  return null;
}

/**
 * A 1 px seam with a 7 px hit zone. Dragging writes `cssVar` (e.g. --side-w) on `target`
 * once per frame and calls `onCommit` once on pointer-up; arrows, Shift+arrows and Home
 * resize from the keyboard; double-click resets to the default.
 */
export function Splitter({
  label,
  value,
  target,
  cssVar,
  onCommit,
  ...g
}: SplitterGeometry & {
  label: string;
  value: number;
  target: RefObject<HTMLElement | null>;
  cssVar: string;
  onCommit: (value: number) => void;
}) {
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const el = target.current;
    if (!el) return;
    e.preventDefault();
    const handle = e.currentTarget;
    const start = g.orientation === "vertical" ? e.clientX : e.clientY;
    handle.setPointerCapture(e.pointerId);
    handle.dataset.dragging = "true";
    el.dataset.dragging = "true";
    let current = value;
    let frame = 0;
    const move = (ev: globalThis.PointerEvent) => {
      const delta = ((g.orientation === "vertical" ? ev.clientX : ev.clientY) - start) * (g.invert ? -1 : 1);
      current = Math.max(g.min, Math.min(g.max, Math.round(value + delta)));
      if (!frame) {
        frame = requestAnimationFrame(() => {
          frame = 0;
          el.style.setProperty(cssVar, `${current}px`);
        });
      }
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
      if (frame) cancelAnimationFrame(frame);
      handle.dataset.dragging = "false";
      delete el.dataset.dragging;
      el.style.setProperty(cssVar, `${current}px`);
      if (current !== value) onCommit(current);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  };
  return (
    <div
      className={`splitter splitter--${g.orientation === "vertical" ? "v" : "h"}`}
      role="separator"
      aria-orientation={g.orientation}
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={g.min}
      aria-valuemax={g.max}
      tabIndex={0}
      data-hint="Drag to resize — double-click resets"
      onPointerDown={onPointerDown}
      onDoubleClick={() => onCommit(Math.max(g.min, Math.min(g.max, g.defaultValue)))}
      onKeyDown={(e: KeyboardEvent<HTMLDivElement>) => {
        const next = splitterKey(e.key, e.shiftKey, value, g);
        if (next === null) return;
        e.preventDefault();
        onCommit(next);
      }}
    />
  );
}
