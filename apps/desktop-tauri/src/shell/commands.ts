// The command palette's lists (Mod+K commands, Mod+P listed files).

import { useMemo } from "react";
import type { PaletteCommand } from "../components/CommandPalette";
import type { DocumentsStore } from "../state/documents";
import { providerTaskTitle } from "../state/tasks";
import type { WorkspaceStore } from "../state/workspace";
import type { ShellContext } from "./actions";
import { MOD } from "./platform";

export type CommandEntry = PaletteCommand & { enabled?: boolean };

/** Every command with its enabled rule, in palette order (disabled ones included). */
export function allCommands(ctx: ShellContext): CommandEntry[] {
  const { client, documents, tasks, settings } = ctx.app;
  const { ready, hasDoc, dark, ws, task, docs, openFolder } = ctx;
  const c: CommandEntry[] = [
    { id: "open", group: "File", label: "Open Folder…", shortcut: `${MOD}+O`, run: () => void openFolder(), enabled: ready },
    { id: "save", group: "File", label: "Save", shortcut: `${MOD}+S`, run: () => documents.save(docs.activePath), enabled: hasDoc },
    { id: "saveAll", group: "File", label: "Save All", run: () => documents.saveAll(), enabled: hasDoc && documents.hasDirty },
    { id: "close", group: "File", label: "Close Folder", run: () => documents.requestCloseFolder(), enabled: !!ws.id },
    { id: "settings", group: "File", label: "Settings…", shortcut: `${MOD}+,`, run: () => ctx.setSettingsOpen(true) },
    { id: "find", group: "Edit", label: "Find", run: () => documents.runCommand("find"), enabled: hasDoc },
    { id: "replace", group: "Edit", label: "Replace", run: () => documents.runCommand("replace"), enabled: hasDoc },
    { id: "format", group: "Edit", label: "Format Document", run: () => documents.runCommand("format"), enabled: hasDoc },
    { id: "sidebar", group: "View", label: "Toggle Side Bar", shortcut: `${MOD}+B`, run: ctx.toggleSidebar },
    { id: "panel", group: "View", label: "Toggle Panel", shortcut: `${MOD}+J`, run: ctx.togglePanel },
    { id: "resetLayout", group: "View", label: "Reset Layout", run: ctx.resetLayout },
    { id: "theme", group: "View", label: dark ? "Switch to Light Theme" : "Switch to Dark Theme", run: () => void settings.setValue("appearance.theme", dark ? "light" : "dark") },
    { id: "run", group: "Run", label: "Run Command…", shortcut: `${MOD}+Shift+R`, run: ctx.focusRun, enabled: !!ws.id },
    { id: "rerun", group: "Run", label: tasks.lastLabel ? `Rerun: ${tasks.lastLabel}` : "Rerun Last", shortcut: "F5", run: () => tasks.rerunLast(), enabled: tasks.canRun && !!tasks.lastLabel },
    // Provider tasks (M2-UI-003): "Build: cargo build", "Test: cargo test"…
    ...task.providerTasks.map((t) => ({
      id: `task:${t.providerId}:${t.id}`,
      group: "Run",
      label: providerTaskTitle(t),
      run: () => tasks.runProvider(t),
      enabled: tasks.canRun,
    })),
    { id: "stop", group: "Run", label: "Stop", shortcut: "Shift+F5", run: () => tasks.stop(), enabled: task.running },
    { id: "restart", group: "Backend", label: "Restart Backend", run: () => void client.restart() },
    { id: "about", group: "Help", label: "About Axiom", run: () => ctx.setAboutOpen(true) },
  ];
  for (const r of ws.recent) {
    c.push({ id: `recent:${r.root}`, group: "Recent", label: `${r.name} — ${r.root}`, run: () => documents.requestOpenFolder(r.root), enabled: ready });
  }
  return c;
}

/** The palette's command list: the enabled commands. */
export function buildCommands(ctx: ShellContext): PaletteCommand[] {
  return allCommands(ctx).filter((x) => x.enabled !== false);
}

/** Quick open: only files the explorer has listed (a workspace index is M4 epic 4.5). */
export function buildFileCommands(workspace: WorkspaceStore, documents: DocumentsStore): PaletteCommand[] {
  return workspace
    .visibleRows()
    .filter((r) => r.node.kind !== "directory")
    .map((r) => ({ id: r.node.path, group: "File", label: r.node.path, run: () => documents.open(r.node.path) }));
}

export function usePaletteCommands(ctx: ShellContext, palette: "commands" | "files" | null): PaletteCommand[] {
  const { client, workspace, documents, tasks, settings } = ctx.app;
  const commands = useMemo(
    () => buildCommands(ctx),
    // Same dependencies as before the split.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctx.ready, ctx.hasDoc, ctx.ws.id, ctx.ws.recent, ctx.dark, ctx.task.running, ctx.task.history, ctx.task.providerTasks, ctx.task.lastProvider, ctx.docs.activePath, documents, tasks, settings, client, ctx.openFolder],
  );
  const files = useMemo(
    () => (palette === "files" ? buildFileCommands(workspace, documents) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [palette, workspace, documents, ctx.ws.treeVersion],
  );
  return palette === "files" ? files : commands;
}
