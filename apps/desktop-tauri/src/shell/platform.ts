// Platform facts the shell needs, defined once.

import { useMemo, useSyncExternalStore } from "react";

export const IS_MAC = navigator.platform.toLowerCase().includes("mac");
export const MOD = IS_MAC ? "⌘" : "Ctrl";

/** The OS colour scheme, live. */
export function useSystemDark(): boolean {
  const mq = useMemo(() => window.matchMedia("(prefers-color-scheme: dark)"), []);
  return useSyncExternalStore(
    (fn) => {
      mq.addEventListener("change", fn);
      return () => mq.removeEventListener("change", fn);
    },
    () => mq.matches,
  );
}
