// The title row (workspace-spec.md §2.1; M1.6-SHELL-003): brand, flat menus, workspace
// switcher, the recessed command field, the task transport and the layout toggles.
// On macOS the row hosts the traffic lights (overlay title bar, D3) and is the drag region
// wherever it is not interactive.

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import * as Icon from "../ui/icons";
import { Menu, type MenuEntry } from "../ui/overlays";
import { IconButton, Keys, Led, RateMeter, type LedState } from "../ui/primitives";
import type { ShellContext } from "./actions";
import type { Layout } from "./layout";
import { IS_MAC, MOD } from "./platform";
import { formatElapsed, METER_BARS, METER_SAMPLE_MS, rateLevels, shortenHome } from "./transport";
import "../styles/components/titlerow.css";

/** Drag-region attribute for non-interactive parts of the row (macOS overlay title bar only). */
const drag = IS_MAC ? { "data-tauri-drag-region": true } : {};

export function TitleRow({ ctx, layout }: { ctx: ShellContext; layout: Pick<Layout, "sidebarVisible" | "panelVisible"> }) {
  const { client, documents, tasks, settings } = ctx.app;
  const { ready, hasDoc, dark, ws, docs, task, prefs, openFolder } = ctx;

  const recent: MenuEntry[] = ws.recent.length
    ? ws.recent.slice(0, 8).map((r) => ({ label: r.name, shortcut: r.root.length > 34 ? "…" + r.root.slice(-32) : r.root, disabled: !ready, onSelect: () => documents.requestOpenFolder(r.root) }))
    : [{ label: "No recent folders", disabled: true }];

  const fileMenu: MenuEntry[] = [
    { label: "Open Folder…", shortcut: `${MOD}+O`, disabled: !ready, onSelect: () => void openFolder() },
    { separator: true },
    { heading: "Recent" },
    ...recent,
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
    <header className="titlerow" data-mac={IS_MAC || undefined} {...drag}>
      <div className="titlerow__brand" {...drag}>
        <Icon.Logo className="titlerow__mark" />
        <span>Axiom</span>
      </div>
      <nav className="titlerow__menus" aria-label="Menus">
        <Menu label="File menu" trigger="File" triggerClassName="titlerow__menu" hint="Open, save and close; settings" items={fileMenu} />
        <Menu
          label="Edit menu"
          trigger="Edit"
          triggerClassName="titlerow__menu"
          hint="Undo, find and select in the editor"
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
          triggerClassName="titlerow__menu"
          hint="Panels, layout and theme"
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
          label="Run menu"
          trigger="Run"
          triggerClassName="titlerow__menu"
          hint="Run, rerun and stop commands"
          items={[
            { label: "Run Command…", shortcut: `${MOD}+Shift+R`, disabled: !ws.id, onSelect: ctx.focusRun },
            { label: "Rerun Last", shortcut: "F5", disabled: !tasks.canRun || task.history.length === 0, onSelect: () => tasks.rerunLast() },
            { label: "Stop", shortcut: "Shift+F5", disabled: !task.running, onSelect: () => tasks.stop() },
          ]}
        />
        <Menu
          label="Help menu"
          trigger="Help"
          triggerClassName="titlerow__menu"
          hint="About Axiom; restart the backend"
          items={[
            { label: "About Axiom", onSelect: () => ctx.setAboutOpen(true) },
            { separator: true },
            { label: "Restart Backend", onSelect: () => void client.restart() },
          ]}
        />
      </nav>
      {ws.id && (
        <>
          <span className="titlerow__sep" aria-hidden="true" />
          <Menu
            label={`Workspace ${ws.name}: switch folder`}
            triggerClassName="titlerow__ws"
            hint="Switch to a recent folder or open another — Mod+O"
            trigger={
              <>
                <span className="titlerow__ws-name">{ws.name}</span>
                <span className="titlerow__ws-path">{shortenHome(ws.root)}</span>
                <Icon.Chevron className="titlerow__ws-chevron" />
              </>
            }
            items={[{ heading: "Recent" }, ...recent, { separator: true }, { label: "Open Folder…", shortcut: `${MOD}+O`, disabled: !ready, onSelect: () => void openFolder() }]}
          />
        </>
      )}
      <span className="titlerow__spacer" {...drag} />
      <button type="button" className="titlerow__cmd" onClick={() => ctx.setPalette("commands")} aria-label="Command palette" data-hint="Search files, run commands — Mod+K">
        <Icon.Search className="titlerow__cmd-icon" />
        <span className="titlerow__cmd-text">Search files, run commands</span>
        <Keys keys={`${IS_MAC ? "⌘" : "Ctrl"} K`} />
      </button>
      <span className="titlerow__cmd-compact">
        <IconButton label="Command palette" shortcut={`${MOD}+K`} hint="Search files, run commands — Mod+K" onClick={() => ctx.setPalette("commands")}>
          <Icon.Search />
        </IconButton>
      </span>
      <span className="titlerow__spacer" {...drag} />
      {ws.id && <Transport ctx={ctx} />}
      <span className="titlerow__toggles">
        <IconButton label="Toggle side bar" shortcut={`${MOD}+B`} pressed={layout.sidebarVisible} onClick={ctx.toggleSidebar}>
          <Icon.Sidebar />
        </IconButton>
        <IconButton label="Toggle panel" shortcut={`${MOD}+J`} pressed={layout.panelVisible} onClick={ctx.togglePanel}>
          <Icon.Panel />
        </IconButton>
        <IconButton label="Settings" shortcut={`${MOD}+,`} onClick={() => ctx.setSettingsOpen(true)}>
          <Icon.Gear />
        </IconButton>
      </span>
    </header>
  );
}

/** True while the window is unfocused or hidden (the loop gate's data-idle): timers skip their updates. */
const idle = () => document.documentElement.dataset.idle === "true";

/**
 * The task transport: LED, label, elapsed time (1 s steps), rate meter (4 Hz), Rerun, Stop.
 * Only what the store knows; no output parsing. No per-frame timers.
 */
function Transport({ ctx }: { ctx: ShellContext }) {
  const { tasks } = ctx.app;
  const { task } = ctx;
  const [now, setNow] = useState(() => Date.now());
  const [levels, setLevels] = useState<number[]>(() => Array<number>(METER_BARS).fill(0));
  const deltas = useRef<number[]>([]);
  const lastSeq = useRef(task.outputSeq);
  const seq = useRef(task.outputSeq);
  seq.current = task.outputSeq;

  useEffect(() => {
    if (!task.running) {
      deltas.current = [];
      setLevels(Array<number>(METER_BARS).fill(0));
      return;
    }
    lastSeq.current = seq.current;
    const clock = setInterval(() => {
      if (!idle()) setNow(Date.now());
    }, 1000);
    const meter = setInterval(() => {
      const d = seq.current - lastSeq.current;
      lastSeq.current = seq.current;
      deltas.current = [...deltas.current, d].slice(-METER_BARS);
      if (idle()) return;
      const next = rateLevels(deltas.current);
      // No output, no render: the bars only change when rows arrive (or age out).
      setLevels((prev) => (prev.every((v, i) => v === next[i]) ? prev : next));
    }, METER_SAMPLE_MS);
    setNow(Date.now());
    return () => {
      clearInterval(clock);
      clearInterval(meter);
    };
  }, [task.running]);

  // An install (M2-UI-002) is the running activity when no task runs: one place shows
  // "something is executing".
  const env = ctx.app.environments;
  // A string snapshot (provider + step), so the label follows the install's progress.
  const installKey = () => {
    const a = env.activeInstall;
    return a ? `${a.providerId}\n${a.progress?.label ?? ""}` : null;
  };
  const install = useSyncExternalStore(env.subscribe, installKey, installKey);
  const installing = !task.running && install !== null ? env.activeInstall : null;
  if (installing) {
    const step = installing.progress?.label;
    const text = `Installing ${installing.name}${step ? `: ${step}` : ""}`;
    return (
      <div className="transport" data-testid="transport" data-state="run" data-activity="install" role="group" aria-label="Task">
        <Led state="run" label="Install running" />
        <span className="transport__label" title={text}>
          {text}
        </span>
        <IconButton label={`Stop installing ${installing.name}`} hint={`Stop installing ${installing.name}`} onClick={() => env.cancelInstall(installing.providerId)}>
          <Icon.Stop />
        </IconButton>
      </div>
    );
  }

  const led: LedState = task.running ? "run" : task.lastOutcome === "succeeded" ? "on" : task.lastOutcome === "failed" ? "err" : "off";
  const label = task.running ? task.label : task.lastOutcome ? task.history[0] ?? "" : "";
  const last = task.history[0];
  return (
    <div className="transport" data-testid="transport" data-state={led} role="group" aria-label="Task">
      <Led state={led} label={task.running ? "Task running" : led === "on" ? "Last task succeeded" : led === "err" ? "Last task failed" : "No task"} />
      <span className={label ? "transport__label" : "transport__label transport__label--idle"} title={label || undefined}>
        {label || "No task"}
      </span>
      {task.running && task.startedAt !== null && <span className="transport__time">{formatElapsed(now - task.startedAt)}</span>}
      <RateMeter levels={levels} label="Output rate" />
      <IconButton label={last ? `Rerun: ${last}` : "Rerun"} shortcut="F5" hint="Rerun the last command — F5" disabled={!tasks.canRun || !last} onClick={() => tasks.rerunLast()}>
        <Icon.Refresh />
      </IconButton>
      <IconButton label="Stop" shortcut="Shift+F5" hint="Stop the task — Shift+F5" disabled={!task.running} onClick={() => tasks.stop()}>
        <Icon.Stop />
      </IconButton>
    </div>
  );
}
