// The bundled IBM Plex files (M1.6-SYSTEM-002, ADR-0013 §6): every @font-face points at
// a file that exists, the licence and provenance ship with them, and the files are
// byte-for-byte the ones SOURCE.md lists (the licence forbids modified "Plex" files).

import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const app = resolve(import.meta.dirname, "..");
const styles = join(app, "src/styles");
const plex = join(app, "src/assets/fonts/ibm-plex");
const fontsCss = readFileSync(join(styles, "fonts.css"), "utf8");

describe("bundled fonts", () => {
  it("every @font-face src exists in src/assets/fonts", () => {
    const urls = [...fontsCss.matchAll(/url\("([^"]+)"\)/g)].map((m) => m[1]!);
    expect(urls.length).toBe(7);
    for (const u of urls) {
      expect(u.startsWith("../assets/fonts/"), u).toBe(true);
      expect(existsSync(resolve(styles, u)), u).toBe(true);
    }
    // and every bundled file is used
    const files = readdirSync(plex).filter((f) => f.endsWith(".woff2"));
    expect(files.sort()).toEqual(urls.map((u) => u.split("/").pop()!).sort());
  });

  it("OFL.txt and SOURCE.md present", () => {
    expect(readFileSync(join(plex, "OFL.txt"), "utf8")).toMatch(/SIL Open Font License, Version 1\.1/);
    expect(readFileSync(join(plex, "OFL.txt"), "utf8")).toMatch(/Reserved Font Name "Plex"/);
    expect(existsSync(join(plex, "SOURCE.md"))).toBe(true);
  });

  it("the files are IBM's, unmodified (SHA-256 as listed in SOURCE.md)", () => {
    const source = readFileSync(join(plex, "SOURCE.md"), "utf8");
    const listed = new Map([...source.matchAll(/`(IBMPlex[\w-]+\.woff2)` \| [\d,]+ \| `([0-9a-f]{64})`/g)].map((m) => [m[1]!, m[2]!]));
    const files = readdirSync(plex).filter((f) => f.endsWith(".woff2"));
    expect(listed.size).toBe(files.length);
    for (const f of files) {
      const sha = createHash("sha256").update(readFileSync(join(plex, f))).digest("hex");
      expect(sha, f).toBe(listed.get(f));
    }
  });

  it("no font package is a runtime dependency (fonts are vendored, never fetched)", () => {
    const pkg = JSON.parse(readFileSync(join(app, "package.json"), "utf8")) as { dependencies?: Record<string, string> };
    expect(Object.keys(pkg.dependencies ?? {}).filter((d) => /fontsource|plex/i.test(d))).toEqual([]);
    expect(fontsCss).not.toMatch(/https?:/);
    expect(dirname(plex)).toBe(join(app, "src/assets/fonts"));
  });
});
