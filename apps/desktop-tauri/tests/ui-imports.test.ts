// ui/ is the design system: it must not depend on stores, the shell, feature views,
// the daemon client or Tauri (frontend-architecture.md §2), so it can be rendered in
// isolation (gallery, tests) and reused by another front end.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const ui = join(import.meta.dirname, "../src/ui");
const FORBIDDEN = [/^\.\.\/(state|shell|components|daemon)(\/|$)/, /^@tauri-apps\//];

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

/** Import specifiers of `text` (static, dynamic and re-exports) that ui/ may not use. */
export function forbiddenImports(text: string): string[] {
  const specs = [...text.matchAll(/(?:import|export)\s[^"']*?from\s*["']([^"']+)["']|import\s*\(\s*["']([^"']+)["']\s*\)|import\s+["']([^"']+)["']/g)].map(
    (m) => m[1] ?? m[2] ?? m[3] ?? "",
  );
  return specs.filter((s) => FORBIDDEN.some((re) => re.test(s)));
}

describe("ui/ import graph", () => {
  it("no file in src/ui imports state, shell, components, daemon or Tauri", () => {
    const bad = files(ui)
      .filter((f) => /\.(ts|tsx)$/.test(f))
      .flatMap((f) => forbiddenImports(readFileSync(f, "utf8")).map((s) => `${f}: ${s}`));
    expect(bad).toEqual([]);
  });

  it("detects an injected bad import (fixture)", () => {
    const fixture = `
      import { useState } from "react";
      import type { AppStores } from "../state/app";
      import { listen } from "@tauri-apps/api/event";
      export { TitleRow } from "../shell/TitleRow";
      const x = import("../components/Explorer");
      import "../styles/components/button.css";`;
    expect(forbiddenImports(fixture)).toEqual(["../state/app", "@tauri-apps/api/event", "../shell/TitleRow", "../components/Explorer"]);
  });
});
