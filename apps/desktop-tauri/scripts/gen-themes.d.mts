// Types for scripts/gen-themes.mjs (plain ESM so Node runs it directly; the token
// guard test imports its colour math).
export declare function hexToRgb(hex: string): [number, number, number];
export declare function rgbToHex(rgb: number[]): string;
export declare function contrast(a: string, b: string): number;
export declare function deltaE2000(a: string, b: string): number;
export interface ThemeInput {
  base: { hue: number; chroma: number };
  accent: { hue: number; chroma: number; lightness: number };
  contrast?: number;
  tone?: string;
}
export declare function build(theme: "dark" | "light", input: ThemeInput): { primitives: [string, string][]; semantic: [string, string][] };
export declare function render(css: string, themes: Record<"dark" | "light", ThemeInput>): string;
