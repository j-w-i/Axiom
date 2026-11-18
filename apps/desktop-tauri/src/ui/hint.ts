// The hint line (workspace-spec.md §2.5; M1.6-SHELL-005), adapted from FL Studio's hint bar:
// the hovered or focused control's `data-hint` ("Action — Shortcut") is shown in the status
// bar, with the shortcut as keycaps. Hints are written once with `Mod` for the platform
// modifier. Event listeners only (no polling); the hint line is never a live region.

import { useEffect, useState } from "react";

const IS_MAC = typeof navigator !== "undefined" && navigator.platform.toLowerCase().includes("mac");

/** A key name a hint may end with (otherwise the suffix stays text). */
const KEY = /^(Mod|Ctrl|Cmd|⌘|Shift|⇧|Alt|⌥|Option|F([1-9]|1[0-2])|Enter|Return|Esc|Escape|Tab|Space|Home|End|PgUp|PgDn|PageUp|PageDown|Up|Down|Left|Right|↑|↓|←|→|Delete|Backspace|.)$/;

/** "Mod" → "Ctrl" or "⌘" (and "Alt" → "⌥" on macOS). */
export function substituteMod(key: string, mac = IS_MAC): string {
  if (key === "Mod") return mac ? "⌘" : "Ctrl";
  if (mac && key === "Alt") return "⌥";
  return key;
}

/**
 * "Rerun the last command — F5" → { what: "Rerun the last command", keys: ["F5"] }.
 * The part after " — " becomes keys only if every token (split on "+" or spaces) is a key
 * name; otherwise the whole hint stays text.
 */
export function parseHint(hint: string, mac = IS_MAC): { what: string; keys: string[] } {
  const i = hint.lastIndexOf(" — ");
  if (i < 0) return { what: hint, keys: [] };
  const tail = hint.slice(i + 3).trim();
  const tokens = tail.split(/[+\s]+/).filter(Boolean);
  if (tokens.length === 0 || !tokens.every((t) => KEY.test(t))) return { what: hint, keys: [] };
  return { what: hint.slice(0, i), keys: tokens.map((t) => substituteMod(t, mac)) };
}

/** The hint of the control under the pointer or with focus; `idle` when there is none. */
export function useHint(idle = ""): string {
  const [hint, setHint] = useState(idle);
  useEffect(() => {
    const pick = (t: EventTarget | null) => {
      const h = t instanceof Element ? t.closest("[data-hint]")?.getAttribute("data-hint") : null;
      setHint(h ?? idle);
    };
    const over = (e: Event) => pick(e.target);
    // Leaving the window (no element is entered) clears it: a hint never sticks.
    const out = (e: PointerEvent) => {
      if (!e.relatedTarget) setHint(idle);
    };
    document.addEventListener("pointerover", over);
    document.addEventListener("focusin", over);
    document.addEventListener("pointerout", out);
    return () => {
      document.removeEventListener("pointerover", over);
      document.removeEventListener("focusin", over);
      document.removeEventListener("pointerout", out);
    };
  }, [idle]);
  return hint;
}
