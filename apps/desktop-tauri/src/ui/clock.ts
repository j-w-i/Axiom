// A 1 Hz clock for elapsed-time displays (the transport, the console header). Ticks only while
// active, and skips its update while the window is idle (the loop gate's data-idle).

import { useEffect, useState } from "react";

/** Elapsed time as mm:ss (minutes keep counting past 59: 75:02). */
export function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

/** Date.now(), refreshed once a second while `active`. */
export function useClock(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const id = setInterval(() => {
      if (document.documentElement.dataset.idle !== "true") setNow(Date.now());
    }, 1000);
    return () => clearInterval(id);
  }, [active]);
  return now;
}
