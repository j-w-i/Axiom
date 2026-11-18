// The task transport's numbers (workspace-spec.md §2.1; M1.6-SHELL-003): elapsed time and
// the output-rate meter. Pure, unit-tested; the title row samples the store at 1 Hz / 4 Hz.

export const METER_BARS = 16;
export const METER_MIN_SCALE = 8;
export const METER_SAMPLE_MS = 250;

export { formatElapsed } from "../ui/clock";

/**
 * Bar levels 0–1 for the rate meter: the last 16 samples of "rows appended in 250 ms",
 * oldest first, normalised to the largest in the window (at least 8 rows, so a trickle
 * stays a trickle). Missing samples are 0.
 */
export function rateLevels(deltas: number[], bars = METER_BARS, minScale = METER_MIN_SCALE): number[] {
  const recent = deltas.slice(-bars);
  const padded = [...Array<number>(bars - recent.length).fill(0), ...recent];
  const scale = Math.max(minScale, ...padded);
  return padded.map((d) => Math.max(0, Math.min(1, d / scale)));
}

/** Home directory shortened to ~ (Linux, macOS, Windows user folders). */
export function shortenHome(path: string): string {
  return path.replace(/^(\/home\/[^/]+|\/Users\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)/, "~");
}
