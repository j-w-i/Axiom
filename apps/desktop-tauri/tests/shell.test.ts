// The command palette list after the App.tsx split (M1.6-SHELL-001) must equal the
// M1.5 list (plus SHELL-002's "Reset Layout", marked below): same ids, groups, labels, shortcuts, order and enabled rules. The
// expected tables below were written from the pre-split App.tsx (commit 75201b8),
// not generated from the new code.

import { describe, expect, it } from "vitest";
import type { ShellContext } from "../src/shell/actions";
import { allCommands, buildCommands } from "../src/shell/commands";
import { MOD } from "../src/shell/platform";

interface Fixture {
  ready: boolean;
  hasDoc: boolean;
  dark: boolean;
  wsId: string;
  history: string[];
  running: boolean;
  canRun: boolean;
  hasDirty: boolean;
}

const RECENT = [{ name: "axiom", root: "/w/axiom" }];
const noop = () => {};

function ctx(f: Fixture): ShellContext {
  return {
    app: {
      client: { restart: async () => {} },
      workspace: {},
      documents: { hasDirty: f.hasDirty },
      tasks: { canRun: f.canRun, lastLabel: f.history[0] ?? "" },
      settings: {},
    } as unknown as ShellContext["app"],
    ready: f.ready,
    hasDoc: f.hasDoc,
    dark: f.dark,
    ws: { id: f.wsId, recent: RECENT } as unknown as ShellContext["ws"],
    docs: { activePath: f.hasDoc ? "src/main.rs" : "" } as unknown as ShellContext["docs"],
    task: { history: f.history, running: f.running, providerTasks: [], lastProvider: null } as unknown as ShellContext["task"],
    prefs: { theme: f.dark ? "dark" : "light" } as unknown as ShellContext["prefs"],
    openFolder: async () => {},
    toggleSidebar: noop,
    togglePanel: noop, toggleDock: () => {},
    resetLayout: noop,
    focusRun: noop,
    showTerminal: noop,
    destination: "files",
    setDestination: noop,
    setSettingsOpen: noop,
    setAboutOpen: noop,
    setPalette: noop,
  };
}

const view = (c: ShellContext) => allCommands(c).map(({ id, group, label, shortcut, enabled }) => ({ id, group, label, shortcut, enabled }));

type Row = [id: string, group: string, label: string, shortcut: string | undefined, enabled: boolean | undefined];
// The tables say "Ctrl+": the modifier is the platform's (⌘ on macOS, where Node's navigator
// reports a Mac and the Mac CI and test hosts run this file).
const rows = (r: Row[]) =>
  r.map(([id, group, label, shortcut, enabled]) => ({ id, group, label, shortcut: shortcut?.replace(/^Ctrl\+/, `${MOD}+`), enabled }));

describe("shell commands", () => {
  it("buildCommands matches the M1.5 command list", () => {
    // 1. Backend not ready, no workspace, dark theme.
    expect(view(ctx({ ready: false, hasDoc: false, dark: true, wsId: "", history: [], running: false, canRun: false, hasDirty: false }))).toEqual(rows([
      ["open", "File", "Open Folder…", "Ctrl+O", false],
      ["save", "File", "Save", "Ctrl+S", false],
      ["saveAll", "File", "Save All", undefined, false],
      ["close", "File", "Close Folder", undefined, false],
      ["settings", "File", "Settings…", "Ctrl+,", undefined],
      ["find", "Edit", "Find", undefined, false],
      ["replace", "Edit", "Replace", undefined, false],
      ["format", "Edit", "Format Document", undefined, false],
      ["chat", "View", "Open Chat", undefined, false], // added by M3-UI-001
      ["newChat", "View", "New Chat", "Ctrl+N", false],
      ["sidebar", "View", "Toggle Side Bar", "Ctrl+B", undefined],
      ["panel", "View", "Toggle Panel", "Ctrl+J", undefined],
      ["dock", "View", "Toggle AI Dock", "Ctrl+L", false], // added by M3-UI-002
      ["terminal", "View", "Show Terminal", "Ctrl+Shift+`", false], // added by M4-TERM-002
      ["resetLayout", "View", "Reset Layout", undefined, undefined], // added by SHELL-002
      ["theme", "View", "Switch to Light Theme", undefined, undefined],
      ["run", "Run", "Run Command…", "Ctrl+Shift+R", false],
      ["rerun", "Run", "Rerun Last", "F5", false],
      ["stop", "Run", "Stop", "Shift+F5", false],
      ["restart", "Backend", "Restart Backend", undefined, undefined],
      ["about", "Help", "About Axiom", undefined, undefined],
      ["recent:/w/axiom", "Recent", "axiom — /w/axiom", undefined, false],
    ]));

    // 2. Ready, no workspace, light theme.
    expect(view(ctx({ ready: true, hasDoc: false, dark: false, wsId: "", history: [], running: false, canRun: false, hasDirty: false }))).toEqual(rows([
      ["open", "File", "Open Folder…", "Ctrl+O", true],
      ["save", "File", "Save", "Ctrl+S", false],
      ["saveAll", "File", "Save All", undefined, false],
      ["close", "File", "Close Folder", undefined, false],
      ["settings", "File", "Settings…", "Ctrl+,", undefined],
      ["find", "Edit", "Find", undefined, false],
      ["replace", "Edit", "Replace", undefined, false],
      ["format", "Edit", "Format Document", undefined, false],
      ["chat", "View", "Open Chat", undefined, true], // added by M3-UI-001
      ["newChat", "View", "New Chat", "Ctrl+N", true],
      ["sidebar", "View", "Toggle Side Bar", "Ctrl+B", undefined],
      ["panel", "View", "Toggle Panel", "Ctrl+J", undefined],
      ["dock", "View", "Toggle AI Dock", "Ctrl+L", false], // added by M3-UI-002
      ["terminal", "View", "Show Terminal", "Ctrl+Shift+`", false], // added by M4-TERM-002
      ["resetLayout", "View", "Reset Layout", undefined, undefined], // added by SHELL-002
      ["theme", "View", "Switch to Dark Theme", undefined, undefined],
      ["run", "Run", "Run Command…", "Ctrl+Shift+R", false],
      ["rerun", "Run", "Rerun Last", "F5", false],
      ["stop", "Run", "Stop", "Shift+F5", false],
      ["restart", "Backend", "Restart Backend", undefined, undefined],
      ["about", "Help", "About Axiom", undefined, undefined],
      ["recent:/w/axiom", "Recent", "axiom — /w/axiom", undefined, true],
    ]));

    // 3. Workspace with a dirty document and a running task (cannot run another).
    const c3 = ctx({ ready: true, hasDoc: true, dark: true, wsId: "w1", history: ["cargo test"], running: true, canRun: false, hasDirty: true });
    expect(view(c3)).toEqual(rows([
      ["open", "File", "Open Folder…", "Ctrl+O", true],
      ["save", "File", "Save", "Ctrl+S", true],
      ["saveAll", "File", "Save All", undefined, true],
      ["close", "File", "Close Folder", undefined, true],
      ["settings", "File", "Settings…", "Ctrl+,", undefined],
      ["find", "Edit", "Find", undefined, true],
      ["replace", "Edit", "Replace", undefined, true],
      ["format", "Edit", "Format Document", undefined, true],
      ["chat", "View", "Open Chat", undefined, true], // added by M3-UI-001
      ["newChat", "View", "New Chat", "Ctrl+N", true],
      ["sidebar", "View", "Toggle Side Bar", "Ctrl+B", undefined],
      ["panel", "View", "Toggle Panel", "Ctrl+J", undefined],
      ["dock", "View", "Toggle AI Dock", "Ctrl+L", true], // added by M3-UI-002
      ["terminal", "View", "Show Terminal", "Ctrl+Shift+`", true], // added by M4-TERM-002
      ["resetLayout", "View", "Reset Layout", undefined, undefined], // added by SHELL-002
      ["theme", "View", "Switch to Light Theme", undefined, undefined],
      ["run", "Run", "Run Command…", "Ctrl+Shift+R", true],
      ["rerun", "Run", "Rerun: cargo test", "F5", false],
      ["stop", "Run", "Stop", "Shift+F5", true],
      ["restart", "Backend", "Restart Backend", undefined, undefined],
      ["about", "Help", "About Axiom", undefined, undefined],
      ["recent:/w/axiom", "Recent", "axiom — /w/axiom", undefined, true],
    ]));

    // The palette shows exactly the enabled ones, in the same order.
    expect(buildCommands(c3).map((c) => c.id)).toEqual(allCommands(c3).filter((c) => c.enabled !== false).map((c) => c.id));
    expect(buildCommands(c3).map((c) => c.id)).not.toContain("rerun");
  });
});
