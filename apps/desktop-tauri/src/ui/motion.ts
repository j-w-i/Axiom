// Motion foundation (motion-system.md §2, §5, §6; M1.6-INTERACTION-001): the motion
// level, a presence helper for enter/exit, and the loop gate that pauses looping
// indicators while the window is hidden or unfocused. No imports from state/,
// shell/, components/ or @tauri-apps (tests/ui-imports.test.ts).

import { useEffect, useRef, useState } from "react";

export type MotionLevel = "standard" | "reduced" | "off";

/** The effective level: the OS reduced-motion preference forces at least "reduced"; "off" stays "off". */
export function resolveMotion(setting: MotionLevel, prefersReduced: boolean): MotionLevel {
  if (setting === "off") return "off";
  return prefersReduced ? "reduced" : setting;
}

/** Applies a level as <html data-motion>, which selects the token column in tokens.css. */
export function applyMotion(level: MotionLevel): void {
  document.documentElement.dataset.motion = level;
}

/** The level currently applied (standard when none is). */
export function currentMotion(): MotionLevel {
  const v = typeof document === "undefined" ? undefined : document.documentElement.dataset.motion;
  return v === "reduced" || v === "off" ? v : "standard";
}

/**
 * Pauses loops while the window is hidden or unfocused: sets <html data-idle="true">
 * on `visibilitychange` (hidden) and window `blur`, removes it on `focus` or visible.
 * base.css pauses every animation under [data-idle]. Returns a disposer.
 */
export function startLoopGate(win: Window = window): () => void {
  const html = win.document.documentElement;
  const idle = () => (html.dataset.idle = "true");
  const awake = () => delete html.dataset.idle;
  const onVisibility = () => (win.document.visibilityState === "hidden" ? idle() : awake());
  win.addEventListener("blur", idle);
  win.addEventListener("focus", awake);
  win.document.addEventListener("visibilitychange", onVisibility);
  if (win.document.visibilityState === "hidden" || !win.document.hasFocus()) idle();
  return () => {
    win.removeEventListener("blur", idle);
    win.removeEventListener("focus", awake);
    win.document.removeEventListener("visibilitychange", onVisibility);
    awake();
  };
}

/**
 * Applies the motion level whenever the setting or the OS preference changes, and runs
 * the loop gate for the app's lifetime. One call site (App).
 */
export function useMotion(setting: MotionLevel): void {
  const [prefersReduced, setPrefersReduced] = useState(() => matchMedia("(prefers-reduced-motion: reduce)").matches);
  useEffect(() => {
    const mq = matchMedia("(prefers-reduced-motion: reduce)");
    const on = () => setPrefersReduced(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  useEffect(() => applyMotion(resolveMotion(setting, prefersReduced)), [setting, prefersReduced]);
  useEffect(() => startLoopGate(), []);
}

// ---- presence ----------------------------------------------------------------------

export interface PresenceState {
  mounted: boolean;
  phase: "in" | "out";
}

/**
 * The presence state machine behind usePresence: mounted while open and for `exitMs`
 * after closing (0 when motion is reduced or off); reopening during the exit returns
 * to "in" at once.
 */
export class Presence {
  private timer: ReturnType<typeof setTimeout> | null = null;
  state: PresenceState;

  constructor(
    open: boolean,
    private readonly onChange: (s: PresenceState) => void,
    private readonly level: () => MotionLevel = currentMotion,
  ) {
    this.state = { mounted: open, phase: open ? "in" : "out" };
  }

  set(open: boolean, exitMs: number): void {
    if (open) {
      this.clear();
      this.update({ mounted: true, phase: "in" });
      return;
    }
    if (!this.state.mounted || this.state.phase === "out") return;
    const ms = this.level() === "standard" ? exitMs : 0;
    if (ms <= 0) {
      this.update({ mounted: false, phase: "out" });
      return;
    }
    this.update({ mounted: true, phase: "out" });
    this.timer = setTimeout(() => {
      this.timer = null;
      this.update({ mounted: false, phase: "out" });
    }, ms);
  }

  dispose(): void {
    this.clear();
  }

  private clear(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  private update(s: PresenceState): void {
    if (s.mounted === this.state.mounted && s.phase === this.state.phase) return;
    this.state = s;
    this.onChange(s);
  }
}

/** Keeps an overlay mounted through its exit animation; `phase` drives data-phase. */
export function usePresence(open: boolean, exitMs: number): PresenceState {
  const [state, setState] = useState<PresenceState>({ mounted: open, phase: open ? "in" : "out" });
  const presence = useRef<Presence | null>(null);
  presence.current ??= new Presence(open, setState);
  useEffect(() => presence.current?.set(open, exitMs), [open, exitMs]);
  useEffect(() => () => presence.current?.dispose(), []);
  return state;
}

// ---- diagnostics (dev and e2e builds only) ---------------------------------------------

if ((import.meta.env.DEV || import.meta.env.MODE === "e2e") && typeof window !== "undefined") {
  (window as unknown as Record<string, unknown>).__axiomMotion = {
    get level() {
      return currentMotion();
    },
    running: () => document.getAnimations().filter((a) => a.playState === "running").length,
  };
}
