// Every interactive element of the title row and the status bar has a data-hint (SHELL-005,
// acceptance 2). The console, tabs and splitters are checked on the live shell by e2e smoke 15.

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ShellContext } from "../src/shell/actions";
import { StatusBar } from "../src/shell/StatusBar";
import { TitleRow } from "../src/shell/TitleRow";
import { Splitter } from "../src/ui/panel";

const noop = () => {};
const ctx = {
  app: {
    client: { restart: async () => {} },
    workspace: {},
    documents: { hasDirty: true },
    tasks: { canRun: false },
    settings: {},
    environments: { subscribe: () => noop, activeInstall: null },
  },
  ready: true,
  hasDoc: true,
  dark: true,
  ws: { id: "w1", name: "axiom", root: "/home/alex/src/axiom", recent: [{ name: "axiom", root: "/home/alex/src/axiom" }] },
  docs: { activePath: "src/main.rs", cursorText: "Ln 1, Col 1", docs: [] },
  task: { running: true, label: "cargo test", statusText: "Running: cargo test", lastOutcome: null, history: ["cargo test"], startedAt: Date.now(), outputSeq: 0 },
  prefs: { theme: "dark" },
  openFolder: async () => {},
  toggleSidebar: noop,
  togglePanel: noop,
  resetLayout: noop,
  focusRun: noop,
  setSettingsOpen: noop,
  setAboutOpen: noop,
  setPalette: noop,
} as unknown as ShellContext;

/** Opening tags of interactive elements without a data-hint attribute. */
function unhinted(html: string): string[] {
  return [...html.matchAll(/<(button|input|textarea|select)\b[^>]*>|<[a-z]+\b[^>]*role="(separator|tab)"[^>]*>/g)]
    .map((m) => m[0])
    .filter((tag) => !/\sdata-hint="[^"]+"/.test(tag));
}

describe("hint coverage", () => {
  it("every control of the title row and the status bar has a hint", () => {
    const html = renderToStaticMarkup(
      <>
        <TitleRow ctx={ctx} layout={{ sidebarVisible: true, panelVisible: true }} />
        <StatusBar ctx={ctx} daemon={{ state: "failed", message: "x" }} />
        <Splitter label="Resize side bar" value={252} min={180} max={480} defaultValue={252} orientation="vertical" target={{ current: null }} cssVar="--side-w" onCommit={noop} />
      </>,
    );
    expect(html.match(/<button\b/g)!.length).toBeGreaterThan(10); // the shell really rendered
    expect(unhinted(html)).toEqual([]);
  });

  it("the hint line is not a live region; the backend and task segments are", () => {
    const html = renderToStaticMarkup(<StatusBar ctx={ctx} daemon={{ state: "ready", serverVersion: "0.1.0", protocolVersion: "0.1", methods: [] }} />);
    expect(html).toMatch(/class="sb__hint" aria-live="off"/);
    expect(html.match(/role="status"/g)).toHaveLength(2); // backend + task
    expect(html).not.toMatch(/<footer[^>]*role=/);
    expect(html).toContain("Connected — </span>axiomd 0.1.0");
  });
});
