// Types for e2e/shots-compare.mjs (plain ESM so Node can run it directly;
// TypeScript and Vitest consume these declarations).

export interface Image {
  width: number;
  height: number;
  /** RGBA, row-major, width * height * 4 bytes. */
  data: Uint8Array;
}

export declare function encodePng(width: number, height: number, rgba: Uint8Array): Buffer;
export declare function decodePng(buf: Buffer): Image;

export interface ImageDiff {
  changed: number;
  total: number;
  ratio: number;
  sameSize: boolean;
  diff: Uint8Array | null;
}
export declare function diffImages(a: Image, b: Image, tolerance?: number): ImageDiff;

export interface StateResult {
  name: string;
  status: "same" | "changed" | "missing-before" | "missing-after" | "size";
  ratio: number;
  changed: number;
  total: number;
  diff: Image | null;
}
export declare function compareDirs(
  dirA: string,
  dirB: string,
  options?: { threshold?: number; tolerance?: number },
): StateResult[];
export declare function writeReport(
  results: StateResult[],
  dirA: string,
  dirB: string,
  outDir: string,
  threshold: number,
): void;
