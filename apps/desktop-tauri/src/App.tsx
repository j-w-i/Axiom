import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { CommandPalette, type PaletteCommand } from "./components/CommandPalette";
import { AboutSheet, ConflictDialog, SettingsSheet, UnsavedDialog } from "./components/Dialogs";
import { EditorArea } from "./components/EditorArea";
import { Explorer } from "./components/Explorer";
import { OutputPanel, type OutputPanelHandle } from "./components/OutputPanel";
import { Welcome } from "./components/Welcome";
import { host, type AppInfo } from "./daemon/tauri-transport";
import type { AppStores } from "./state/app";
import * as Icon from "./ui/icons";
import { Banner, IconButton, Menu, Splitter } from "./ui/primitives";

const IS_MAC = navigator.platform.toLowerCase().includes("mac");
const MOD = IS_MAC ? "⌘" : "Ctrl";

/** Shell-local layout (per machine, like the Qt shell's window QSettings). */
interface Layout { sidebar: number; panel: number; sidebarVisible: boolean; panelVisible: boolean }
const LAYOUT_KEY = "axiom.layout.v1";
function loadLayout(): Layout {
  const d: Layout = { sidebar: 260, panel: 220, sidebarVisible: window.innerWidth >= 900, panelVisible: true };
  try {
    return { ...d, ...(JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? "{}") as Partial<Layout>) };
  } catch {
    return d;
  }
}

function useSystemDark(): boolean {
  const mq = useMemo(() => window.matchMedia("(prefers-color-scheme: dark)"), []);
  return useSyncExternalStore(
    (fn) => {
      mq.addEventListener("change", fn);
      return () => mq.removeEventListener("change", fn);
    },
    () => mq.matches,
  );
}

export function App({ app }: { app: AppStores }) {
  const { client, workspace, documents, tasks, settings } = app;
  const daemon = useSyncExternalStore(
    (fn) => client.onState(fn),
    () => client.state,
  );
  const ws = useSyncExternalStore(workspace.subscribe, workspace.getSnapshot);
  const docs = useSyncExternalStore(documents.subscribe, documents.getSnapshot);
  const task = useSyncExternalStore(tasks.subscribe, tasks.getSnapshot);
  const prefs = useSyncExternalStore(settings.subscribe, settings.getSnapshot);
  const systemDark = useSystemDark();
  const dark = prefs.theme === "dark" || (prefs.theme === "system" && systemDark);

  const [layout, setLayout] = useState<Layout>(loadLayout);
  useEffect(() => localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout)), [layout]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [palette, setPalette] = useState<"commands" | "files" | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const panelRef = useRef<OutputPanelHandle>(null);

  const ready = daemon.state === "ready";

  // ---- theme & editor appearance --------------------------------------------
  useEffect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
  }, [dark]);
  useEffect(() => {
    documents.setAppearance({
      theme: dark ? "dark" : "light",
      fontSize: prefs.editorFontSize,
      fontFamily: prefs.editorFontFamily || getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim(),
      wordWrap: prefs.editorWordWrap,
    });
  }, [documents, dark, prefs.editorFontSize, prefs.editorFontFamily, prefs.editorWordWrap, docs.editorReady]);

  useEffect(() => {
    document.title = ws.name ? `${ws.name} — Axiom` : "Axiom";
  }, [ws.name]);

  useEffect(() => {
    void host.appInfo().then(setInfo).catch(() => {});
  }, []);

  // ---- crash recovery: prove the page is alive, and keep unsaved text in the host ----
  useEffect(() => {
    documents.recovery = {
      snapshotPut: (path, text) => void host.snapshotPut(path, text).catch(() => {}),
      snapshotClear: (path) => void host.snapshotClear(path).catch(() => {}),
      state: () => host.recoveryState(),
    };
    let alive = true;
    const beat = () => {
      if (alive) void host.heartbeat().catch(() => {});
    };
    beat();
    const id = setInterval(beat, 2000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [documents]);

  // ---- quit flow: the window's close button asks the UI first ----------------
  useEffect(() => {
    documents.openExternal = (url) => void host.openExternal(url);
    documents.onQuitConfirmed = () => {
      void (async () => {
        await workspace.flushState();
        await documents.saveTabState();
        await host.quit();
      })();
    };
    return client.onCloseRequested(() => documents.requestQuit());
  }, [client, documents, workspace]);

  // ---- actions --------------------------------------------------------------
  const openFolder = useCallback(async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Open Folder" });
    if (typeof picked === "string") documents.requestOpenFolder(picked);
  }, [documents]);

  const toggleSidebar = () => setLayout((l) => ({ ...l, sidebarVisible: !l.sidebarVisible }));
  const togglePanel = () => setLayout((l) => ({ ...l, panelVisible: !l.panelVisible }));
  const focusRun = () => {
    setLayout((l) => ({ ...l, panelVisible: true }));
    setTimeout(() => panelRef.current?.focusCommand(), 0);
  };

  /** One implementation per action: the native menu, the in-window menus and the
   * command palette all call exactly these. */
  const runMenuAction = (id: string) => {
    switch (id) {
      case "open-folder": if (ready) void openFolder(); break;
      case "save": if (hasDoc) documents.save(docs.activePath); break;
      case "save-all": if (hasDoc) documents.saveAll(); break;
      case "close-folder": if (ws.id) documents.requestCloseFolder(); break;
      case "settings": setSettingsOpen(true); break;
      case "find": if (hasDoc) documents.runCommand("find"); break;
      case "toggle-sidebar": setLayout((l) => ({ ...l, sidebarVisible: !l.sidebarVisible })); break;
      case "toggle-panel": setLayout((l) => ({ ...l, panelVisible: !l.panelVisible })); break;
      case "run-command":
        setLayout((l) => ({ ...l, panelVisible: true }));
        setTimeout(() => panelRef.current?.focusCommand(), 0);
        break;
      case "rerun": if (tasks.canRun) tasks.rerunLast(); break;
      case "stop": tasks.stop(); break;
      case "about": setAboutOpen(true); break;
      default: break;
    }
  };
  const hasDoc = docs.docs.length > 0 && docs.editorReady;

  const commands: PaletteCommand[] = useMemo(() => {
    const c: (PaletteCommand & { enabled?: boolean })[] = [
      { id: "open", group: "File", label: "Open Folder…", shortcut: `${MOD}+O`, run: () => void openFolder(), enabled: ready },
      { id: "save", group: "File", label: "Save", shortcut: `${MOD}+S`, run: () => documents.save(docs.activePath), enabled: hasDoc },
      { id: "saveAll", group: "File", label: "Save All", run: () => documents.saveAll(), enabled: hasDoc && documents.hasDirty },
      { id: "close", group: "File", label: "Close Folder", run: () => documents.requestCloseFolder(), enabled: !!ws.id },
      { id: "settings", group: "File", label: "Settings…", shortcut: `${MOD}+,`, run: () => setSettingsOpen(true) },
      { id: "find", group: "Edit", label: "Find", run: () => documents.runCommand("find"), enabled: hasDoc },
      { id: "replace", group: "Edit", label: "Replace", run: () => documents.runCommand("replace"), enabled: hasDoc },
      { id: "format", group: "Edit", label: "Format Document", run: () => documents.runCommand("format"), enabled: hasDoc },
      { id: "sidebar", group: "View", label: "Toggle Side Bar", shortcut: `${MOD}+B`, run: toggleSidebar },
      { id: "panel", group: "View", label: "Toggle Panel", shortcut: `${MOD}+J`, run: togglePanel },
      { id: "theme", group: "View", label: dark ? "Switch to Light Theme" : "Switch to Dark Theme", run: () => void settings.setValue("appearance.theme", dark ? "light" : "dark") },
      { id: "run", group: "Run", label: "Run Command…", shortcut: `${MOD}+Shift+R`, run: focusRun, enabled: !!ws.id },
      { id: "rerun", group: "Run", label: task.history[0] ? `Rerun: ${task.history[0]}` : "Rerun Last", shortcut: "F5", run: () => tasks.rerunLast(), enabled: tasks.canRun && task.history.length > 0 },
      { id: "stop", group: "Run", label: "Stop", shortcut: "Shift+F5", run: () => tasks.stop(), enabled: task.running },
      { id: "restart", group: "Backend", label: "Restart Backend", run: () => void client.restart() },
      { id: "about", group: "Help", label: "About Axiom", run: () => setAboutOpen(true) },
    ];
    for (const r of ws.recent) {
      c.push({ id: `recent:${r.root}`, group: "Recent", label: `${r.name} — ${r.root}`, run: () => documents.requestOpenFolder(r.root), enabled: ready });
    }
    return c.filter((x) => x.enabled !== false);
  }, [ready, hasDoc, ws.id, ws.recent, dark, task.running, task.history, docs.activePath, documents, tasks, settings, client, openFolder]);

  const fileCommands: PaletteCommand[] = useMemo(() => {
    if (palette !== "files") return [];
    return workspace
      .visibleRows()
      .filter((r) => r.node.kind !== "directory")
      .map((r) => ({ id: r.node.path, group: "File", label: r.node.path, run: () => documents.open(r.node.path) }));
  }, [palette, workspace, documents, ws.treeVersion]);

  // ---- native menu (macOS): the menu bar dispatches the same actions as the
  // in-window menus and the command palette, as `axiom://menu` events (T-07).
  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void listen<string>("axiom://menu", (event) => runMenuAction(event.payload))
      .then((un) => {
        if (cancelled) un();
        else stop = un;
      })
      .catch(() => {
        /* not a Tauri window (tests, browser): no menu events */
      });
    return () => {
      cancelled = true;
      stop?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, hasDoc, ws.id, docs.activePath, dark, prefs.theme]);

  // ---- application shortcuts (Monaco keeps its own while focused) -------------
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = IS_MAC ? e.metaKey : e.ctrlKey;
      const k = e.key.toLowerCase();
      let handled = true;
      if (mod && !e.shiftKey && k === "k") setPalette("commands");
      else if (mod && !e.shiftKey && k === "p") setPalette("files");
      else if (mod && !e.shiftKey && k === "o") { if (ready) void openFolder(); }
      else if (mod && !e.shiftKey && k === "b") toggleSidebar();
      else if (mod && !e.shiftKey && k === "j") togglePanel();
      else if (mod && !e.shiftKey && k === ",") setSettingsOpen(true);
      else if (mod && e.shiftKey && k === "r") { if (ws.id) focusRun(); }
      else if (mod && !e.shiftKey && k === "s") { if (hasDoc) documents.save(docs.activePath); }
      else if (mod && !e.shiftKey && k === "w") { if (docs.activePath) documents.close(docs.activePath); }
      else if (mod && e.key === "PageUp") documents.cycle(-1);
      else if (mod && e.key === "PageDown") documents.cycle(1);
      else if (mod && !e.shiftKey && k === "q") documents.requestQuit();
      else if (mod && k === "tab") documents.cycle(e.shiftKey ? -1 : 1);
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
  }, [ready, ws.id, hasDoc, docs.activePath, documents, tasks, openFolder]);

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
    { label: "Settings…", shortcut: `${MOD}+,`, onSelect: () => setSettingsOpen(true) },
    { label: "About Axiom", onSelect: () => setAboutOpen(true) },
    { separator: true },
    { label: "Quit", shortcut: `${MOD}+Q`, onSelect: () => documents.requestQuit() },
  ];

  const daemonLabel =
    daemon.state === "ready" ? `Connected — axiomd ${daemon.serverVersion}`
    : daemon.state === "starting" ? "Connecting…"
    : "Backend stopped";

  return (
    <div className="shell">
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
            { label: "Toggle Side Bar", shortcut: `${MOD}+B`, onSelect: toggleSidebar },
            { label: "Toggle Panel", shortcut: `${MOD}+J`, onSelect: togglePanel },
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
            { label: "About Axiom", onSelect: () => setAboutOpen(true) },
            { separator: true },
            { label: "Restart Backend", onSelect: () => void client.restart() },
          ]}
        />
        <Menu
          label="Run menu"
          trigger="Run"
          items={[
            { label: "Run Command…", shortcut: `${MOD}+Shift+R`, disabled: !ws.id, onSelect: focusRun },
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
        <button type="button" className="btn btn--quiet btn--sm" onClick={() => setPalette("commands")} aria-label="Command palette">
          <Icon.Search /> <span style={{ color: "var(--text-faint)" }}>{MOD}+K</span>
        </button>
        <IconButton label={`Toggle side bar (${MOD}+B)`} onClick={toggleSidebar} aria-pressed={layout.sidebarVisible}>
          <Icon.Sidebar />
        </IconButton>
        <IconButton label={`Toggle panel (${MOD}+J)`} onClick={togglePanel} aria-pressed={layout.panelVisible}>
          <Icon.Panel />
        </IconButton>
        <IconButton label="Settings" onClick={() => setSettingsOpen(true)}>
          <Icon.Gear />
        </IconButton>
      </header>

      <div>
        {daemon.state === "failed" && (
          <Banner variant="danger" action="Restart backend" onAction={() => void client.restart()}>
            {daemon.message}
          </Banner>
        )}
        {docs.banner && (
          <Banner variant="warning" action="Dismiss" onAction={() => documents.dismissBanner()}>
            {docs.banner}
          </Banner>
        )}
        {ws.error && (
          <Banner variant="warning" action="Dismiss" onAction={() => workspace.dismissError()}>
            {ws.error}
          </Banner>
        )}
        {prefs.error && (
          <Banner variant="warning" action="Dismiss" onAction={() => settings.dismissError()}>
            {prefs.error}
          </Banner>
        )}
      </div>

      <div className="workbench">
        {layout.sidebarVisible && (
          <>
            <aside className="sidebar" style={{ width: layout.sidebar }} aria-label="Explorer">
              <div className="section-header">
                <span className="section-header__title">{ws.name || "Explorer"}</span>
                {ws.id && (
                  <IconButton label="Refresh explorer" onClick={() => workspace.applyChanges([""])}>
                    <Icon.Refresh />
                  </IconButton>
                )}
              </div>
              <Explorer workspace={workspace} onOpenFile={(p) => documents.open(p)} />
            </aside>
            <Splitter
              orientation="vertical"
              label="Resize side bar"
              size={layout.sidebar}
              min={160}
              max={600}
              onResize={(sidebar) => setLayout((l) => ({ ...l, sidebar }))}
            />
          </>
        )}
        <main className="main">
          <div style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 120, position: "relative" }}>
            {ws.id || ws.opening ? (
              <EditorArea documents={documents} mod={MOD} />
            ) : (
              <Welcome recent={ws.recent} ready={ready} onOpenFolder={() => void openFolder()} onOpenRecent={(r) => documents.requestOpenFolder(r)} mod={MOD} />
            )}
          </div>
          {layout.panelVisible && (
            <>
              <Splitter
                orientation="horizontal"
                label="Resize panel"
                size={layout.panel}
                min={100}
                max={Math.max(140, window.innerHeight - 260)}
                invert
                onResize={(panel) => setLayout((l) => ({ ...l, panel }))}
              />
              <div style={{ height: layout.panel, display: "flex", flexDirection: "column" }}>
                <OutputPanel ref={panelRef} tasks={tasks} workspace={workspace} ready={ready} />
              </div>
            </>
          )}
        </main>
      </div>

      <footer className="statusbar" role="status" aria-label={daemonLabel}>
        <span className="status-dot" data-state={daemon.state} data-testid="status">
          {daemonLabel}
        </span>
        <span className="statusbar__spacer" />
        {task.statusText && (
          <span className={task.lastOutcome === "failed" ? "statusbar__failed" : undefined} data-testid="task-status">
            {task.running && <span className="spinner" style={{ display: "inline-block", verticalAlign: "-2px", marginRight: 6 }} />}
            {task.statusText}
          </span>
        )}
        {docs.activePath && docs.cursorText && <span>{docs.cursorText}</span>}
        {docs.activePath && <span>UTF-8</span>}
      </footer>

      <ConflictDialog documents={documents} />
      <UnsavedDialog documents={documents} />
      <SettingsSheet settings={settings} open={settingsOpen} onOpenChange={setSettingsOpen} />
      <AboutSheet open={aboutOpen} onOpenChange={setAboutOpen} info={info} serverVersion={daemon.state === "ready" ? daemon.serverVersion : null} />
      <CommandPalette
        open={palette !== null}
        onOpenChange={(o) => !o && setPalette(null)}
        commands={palette === "files" ? fileCommands : commands}
        placeholder={palette === "files" ? "Go to a listed file…" : "Type a command…"}
      />
    </div>
  );
}
