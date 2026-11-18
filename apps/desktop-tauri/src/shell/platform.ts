// Platform facts the shell needs, defined once.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useMemo, useSyncExternalStore } from "react";

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

/**
 * D3 (M1.6-SURFACES-003): on Windows and Linux the native title bar follows the theme setting
 * ("system" hands it back to the OS). macOS uses the overlay title bar (SHELL-003) and needs no
 * call. The outcome is kept on <html data-native-theme> ("light" | "dark" | "system" | "error").
 */
export function useNativeTitleBarTheme(theme: "system" | "light" | "dark"): void {
  useEffect(() => {
    if (IS_MAC) return;
    const html = document.documentElement;
    getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .then(
        () => (html.dataset.nativeTheme = theme),
        () => (html.dataset.nativeTheme = "error"),
      );
  }, [theme]);
}
