// The title row: brand, the five menus, the workspace crumb, the palette button,
// the layout toggles and the settings button.

import * as Icon from "../ui/icons";
import { IconButton, Menu } from "../ui/primitives";
import type { ShellContext } from "./actions";
import type { Layout } from "./layout";
import { MOD } from "./platform";

export function TitleRow({ ctx, layout }: { ctx: ShellContext; layout: Pick<Layout, "sidebarVisible" | "panelVisible"> }) {
  const { client, documents, tasks, settings } = ctx.app;
  const { ready, hasDoc, dark, ws, docs, task, prefs, openFolder } = ctx;

  const fileMenu = [
    { label: "Open Folder…", shortcut: `${MOD}+O`, disabled: !ready, onSelect: () => void openFolder() },
    { separator: true },
    { heading: "Recent" },
    ...(ws.recent.length
      ? ws.recent.slice(0, 8).map((r) => ({ label: r.name, shortcut: r.root.length > 34 ? "…" + r.root.slice(-32) : r.root, disabled: !ready, onSelect: () => documents.requestOpenFolder(r.root) }))
      : [{ label: "No recent folders", disabled: true }]),
    { separator: true },
    { label: "Save", shortcut: `${MOD}+S`, disabled: !hasDoc, onSelect: () => documents.save(docs.activePath) },
    { label: "Save All", disabled: !hasDoc || !documents.hasDirty, onSelect: () => documents.saveAll() },
    { label: "Close Folder", disabled: !ws.id, onSelect: () => documents.requestCloseFolder() },
    { separator: true },
    { label: "Settings…", shortcut: `${MOD}+,`, onSelect: () => ctx.setSettingsOpen(true) },
    { label: "About Axiom", onSelect: () => ctx.setAboutOpen(true) },
    { separator: true },
    { label: "Quit", shortcut: `${MOD}+Q`, onSelect: () => documents.requestQuit() },
  ];

  return (
    <header className="titlebar">
      <div className="titlebar__brand">
        <Icon.Logo className="titlebar__logo" />
        <span>Axiom</span>
      </div>
      <Menu label="File menu" trigger="File" items={fileMenu} />
      <Menu
        label="Edit menu"
        trigger="Edit"
        items={[
          { label: "Undo", disabled: !hasDoc, onSelect: () => documents.runCommand("undo") },
          { label: "Redo", disabled: !hasDoc, onSelect: () => documents.runCommand("redo") },
          { separator: true },
          { label: "Find", disabled: !hasDoc, onSelect: () => documents.runCommand("find") },
          { label: "Replace", disabled: !hasDoc, onSelect: () => documents.runCommand("replace") },
          { separator: true },
          { label: "Select All", disabled: !hasDoc, onSelect: () => documents.runCommand("selectAll") },
        ]}
      />
      <Menu
        label="View menu"
        trigger="View"
        items={[
          { label: "Toggle Side Bar", shortcut: `${MOD}+B`, onSelect: ctx.toggleSidebar },
          { label: "Toggle Panel", shortcut: `${MOD}+J`, onSelect: ctx.togglePanel },
          { label: "Reset Layout", onSelect: ctx.resetLayout },
          { separator: true },
          { label: "Light Theme", disabled: !dark, onSelect: () => void settings.setValue("appearance.theme", "light") },
          { label: "Dark Theme", disabled: dark, onSelect: () => void settings.setValue("appearance.theme", "dark") },
          { label: "Follow System Theme", disabled: prefs.theme === "system", onSelect: () => void settings.setValue("appearance.theme", "system") },
        ]}
      />
      <Menu
        label="Help menu"
        trigger="Help"
        items={[
          { label: "About Axiom", onSelect: () => ctx.setAboutOpen(true) },
          { separator: true },
          { label: "Restart Backend", onSelect: () => void client.restart() },
        ]}
      />
      <Menu
        label="Run menu"
        trigger="Run"
        items={[
          { label: "Run Command…", shortcut: `${MOD}+Shift+R`, disabled: !ws.id, onSelect: ctx.focusRun },
          { label: "Rerun Last", shortcut: "F5", disabled: !tasks.canRun || task.history.length === 0, onSelect: () => tasks.rerunLast() },
          { label: "Stop", shortcut: "Shift+F5", disabled: !task.running, onSelect: () => tasks.stop() },
        ]}
      />
      {ws.name && (
        <span className="titlebar__crumb" title={ws.root}>
          / {ws.name}
        </span>
      )}
      <div className="titlebar__spacer" />
      <button type="button" className="btn btn--quiet btn--sm" onClick={() => ctx.setPalette("commands")} aria-label="Command palette">
        <Icon.Search /> <span style={{ color: "var(--text-faint)" }}>{MOD}+K</span>
      </button>
      <IconButton label={`Toggle side bar (${MOD}+B)`} onClick={ctx.toggleSidebar} aria-pressed={layout.sidebarVisible}>
        <Icon.Sidebar />
      </IconButton>
      <IconButton label={`Toggle panel (${MOD}+J)`} onClick={ctx.togglePanel} aria-pressed={layout.panelVisible}>
        <Icon.Panel />
      </IconButton>
      <IconButton label="Settings" onClick={() => ctx.setSettingsOpen(true)}>
        <Icon.Gear />
      </IconButton>
    </header>
  );
}
