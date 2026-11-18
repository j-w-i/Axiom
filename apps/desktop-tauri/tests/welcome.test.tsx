// Welcome and the empty states (SHELL-007; workspace-spec.md §6).

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Welcome, WAITING } from "../src/components/Welcome";
import { emptyShortcuts } from "../src/components/EditorArea";

const recent = [
  { name: "axiom", root: "/home/alex/src/axiom", lastOpenedMs: 2 },
  { name: "notes", root: "/tmp/notes", lastOpenedMs: 1 },
];
const noop = () => {};

describe("welcome", () => {
  it("ready=false: Open Folder… is aria-disabled and says why; recent rows are disabled", () => {
    const html = renderToStaticMarkup(<Welcome recent={recent} ready={false} onOpenFolder={noop} onOpenRecent={noop} mod="Ctrl" />);
    const open = html.match(/<button[^>]*data-testid="welcome-open"[^>]*>/)?.[0] ?? "";
    expect(open).toContain('aria-disabled="true"');
    expect(open).not.toMatch(/\sdisabled=""/); // still focusable, so the tooltip can show
    expect(open).toContain(`data-hint="${WAITING}"`);
    expect(open).toContain('data-state="closed"'); // wrapped in the tooltip trigger
    expect(html.match(/class="welcome__item" disabled=""/g)).toHaveLength(2);
    expect(html).toContain("Open a folder to start");
    expect(html).toContain("Axiom opens folders through its backend, axiomd.");
    expect(html).toContain('<span class="welcome__item-path">~/src/axiom</span>'); // home shortened
    expect(html).not.toContain("!");
  });

  it("ready=true: plain enabled button with its shortcut hint, no tooltip wrapper", () => {
    const html = renderToStaticMarkup(<Welcome recent={[]} ready onOpenFolder={noop} onOpenRecent={noop} mod="Ctrl" />);
    const open = html.match(/<button[^>]*data-testid="welcome-open"[^>]*>/)?.[0] ?? "";
    expect(open).not.toContain("aria-disabled");
    expect(open).toContain('data-hint="Open a folder — Mod+O"');
    expect(html).not.toContain("Recent"); // no list without recent folders
  });

  it("no-document shortcuts: the four rows, platform keycaps", () => {
    expect(emptyShortcuts("Ctrl").map(([l]) => l)).toEqual(["Command palette", "Go to file", "Run command", "Toggle console"]);
    expect(emptyShortcuts("Ctrl")[2]![1]).toEqual(["Ctrl", "Shift", "R"]);
    expect(emptyShortcuts("⌘")[2]![1]).toEqual(["⌘", "⇧", "R"]);
  });
});
