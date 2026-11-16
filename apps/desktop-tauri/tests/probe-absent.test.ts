// The benchmark probe (src/e2e-probe.ts) must not exist in a production
// bundle: it patches the React DevTools hook and measures input. This test
// builds the UI the way `pnpm run build` does and greps the output.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const app = resolve(import.meta.dirname, "..");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

function build(mode?: string): string {
  const out = mkdtempSync(join(tmpdir(), "axiom-probe-"));
  const args = ["exec", "vite", "build", "--outDir", out, "--emptyOutDir", ...(mode ? ["--mode", mode] : [])];
  execFileSync("pnpm", args, { cwd: app, stdio: "pipe" });
  return out;
}

// (react-dom itself mentions __REACT_DEVTOOLS_GLOBAL_HOOK__, so that name proves nothing.)
const MARKERS = ["__axiomFrames", "__axiomMarks", "__axiomCommits"];
const text = (dir: string) => files(dir).filter((f) => /\.(js|html)$/.test(f)).map((f) => readFileSync(f, "utf8")).join("\n");

describe("benchmark probe", () => {
  it("production bundle contains no e2e probe", () => {
    const out = build();
    try {
      const all = text(out);
      for (const m of MARKERS) expect(all.includes(m), `${m} found in the production bundle`).toBe(false);
    } finally {
      rmSync(out, { recursive: true, force: true });
    }
  }, 120_000);

  it("the e2e-mode bundle does contain it (so the test above can fail)", () => {
    const out = build("e2e");
    try {
      const all = text(out);
      expect(all.includes("__axiomFrames")).toBe(true);
      expect(all.includes("__axiomMarks")).toBe(true);
    } finally {
      rmSync(out, { recursive: true, force: true });
    }
  }, 120_000);
});
