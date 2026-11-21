// Application shortcuts (Monaco keeps its own while focused): one capture-phase
// handler on window, so they win over focused controls.

import { useEffect } from "react";
import type { ShellContext } from "./actions";
import { IS_MAC } from "./platform";
import { requestSearchFocus } from "../components/Search";

export function useAppShortcuts(ctx: ShellContext): void {
  const { documents, tasks } = ctx.app;
  const { ready, hasDoc, openFolder } = ctx;
  const wsId = ctx.ws.id;
  const activePath = ctx.docs.activePath;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = IS_MAC ? e.metaKey : e.ctrlKey;
      const k = e.key.toLowerCase();
      let handled = true;
      if (mod && !e.shiftKey && k === "k") ctx.setPalette("commands");
      else if (mod && !e.shiftKey && k === "p") ctx.setPalette("files");
      else if (mod && !e.shiftKey && k === "o") { if (ready) void openFolder(); }
      else if (mod && !e.shiftKey && k === "b") ctx.toggleSidebar();
      else if (mod && !e.shiftKey && k === "j") ctx.togglePanel();
      else if (mod && !e.shiftKey && k === "l") { if (wsId) ctx.toggleDock(); }
      else if (mod && !e.shiftKey && k === ",") ctx.setSettingsOpen(true);
      else if (mod && e.shiftKey && k === "r") { if (wsId) ctx.focusRun(); }
      else if (mod && !e.shiftKey && k === "s") { if (hasDoc) documents.save(activePath); }
      else if (mod && !e.shiftKey && k === "w") { if (activePath) documents.close(activePath); }
      // Mod+Shift+PageUp/PageDown reorder the focused tab (INTERACTION-004): left to the tab.
      else if (mod && !e.shiftKey && e.key === "PageUp") documents.cycle(-1);
      else if (mod && !e.shiftKey && e.key === "PageDown") documents.cycle(1);
      else if (mod && !e.shiftKey && k === "q") documents.requestQuit();
      else if (mod && !e.shiftKey && k === "n" && ctx.destination === "chat") ctx.app.agent.newChat();
      else if (mod && k === "tab") documents.cycle(e.shiftKey ? -1 : 1);
      // Region focus (EDITOR-002; workspace-spec.md §1). Physical keys (e.code), so the chords
      // work on any layout. Monaco has no binding of its own for them (see the handoff table).
      else if (mod && !e.shiftKey && !e.altKey && e.code === "Digit0") focusRegion("files", ctx);
      else if (mod && !e.shiftKey && !e.altKey && e.code === "Digit1") focusRegion("editor", ctx);
      // M4-UX-002: split the editor (Mod+\), and the right group (Mod+2).
      else if (mod && !e.shiftKey && !e.altKey && e.code === "Backslash") { if (activePath) documents.splitRight(); }
      else if (mod && !e.shiftKey && !e.altKey && e.code === "Digit2") { if (!documents.focusSecondary()) handled = false; }
      else if (mod && !e.shiftKey && !e.altKey && e.code === "Backquote") focusRegion("console", ctx);
      // M4-TERM-002: the Terminal view (Mod+` stays the console region, EDITOR-002).
      else if (mod && e.shiftKey && !e.altKey && e.code === "Backquote") { if (wsId) ctx.showTerminal(); }
      // M4-UX-001: the Search page, its query focused.
      else if (mod && e.shiftKey && !e.altKey && k === "f") { if (wsId) { ctx.setDestination("search"); requestSearchFocus(); } }
      else if (e.key === "F5" && !e.shiftKey) { if (tasks.canRun) tasks.rerunLast(); }
      else if (e.key === "F5" && e.shiftKey) tasks.stop();
      else handled = false;
      if (handled) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // Same dependencies as before the split: the setters and toggles are stable in effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, wsId, hasDoc, activePath, documents, tasks, openFolder, ctx.destination]);
}

/** Moves keyboard focus to a region, showing it first if it is hidden. */
export function focusRegion(region: "files" | "editor" | "console", ctx: ShellContext): void {
  if (region === "console") {
    ctx.focusRun(); // shows the console and focuses its prompt
    return;
  }
  if (region === "files") {
    const tree = () => document.querySelector<HTMLElement>('[role="tree"]');
    if (tree()) tree()!.focus();
    else if (!document.querySelector('[data-region="files"]')) {
      ctx.toggleSidebar();
      requestAnimationFrame(() => tree()?.focus());
    }
    return;
  }
  focusEditor();
}

/** Focuses Monaco's input when a document is open (what a click does), otherwise the active tab. */
export function focusEditor(): void {
  const input = document.querySelector<HTMLElement>('[data-region="editor"] .monaco-editor textarea');
  if (input && !input.closest("[hidden]")) input.focus();
  else document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
}
