// The component gallery is a dev tool: a normal `vite build` (what the app embeds)
// must not contain it.

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { expect, it } from "vitest";

const app = resolve(import.meta.dirname, "..");
const files = (dir: string): string[] =>
  readdirSync(dir).flatMap((f) => (statSync(join(dir, f)).isDirectory() ? files(join(dir, f)) : [join(dir, f)]));

it("the production build contains no gallery code", () => {
  const out = mkdtempSync(join(tmpdir(), "axiom-gallery-"));
  try {
    execFileSync("pnpm", ["exec", "vite", "build", "--outDir", out, "--emptyOutDir"], { cwd: app, stdio: "pipe" });
    expect(existsSync(join(out, "gallery.html"))).toBe(false);
    const text = files(out).filter((f) => /\.(js|html|css)$/.test(f)).map((f) => readFileSync(f, "utf8")).join("\n");
    expect(text.includes("axiom-component-gallery")).toBe(false);
    expect(text.includes("Component gallery")).toBe(false);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
