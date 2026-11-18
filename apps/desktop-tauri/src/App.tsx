import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { ConflictDialog, SettingsDialog, UnsavedDialog } from "./components/Dialogs";
import { host, type AppInfo } from "./daemon/tauri-transport";
import { useNativeMenu, type ShellContext } from "./shell/actions";
import { usePaletteCommands } from "./shell/commands";
import { useNativeTitleBarTheme, useSystemDark } from "./shell/platform";
import { useAppShortcuts } from "./shell/shortcuts";
import { StatusBar } from "./shell/StatusBar";
import { TitleRow } from "./shell/TitleRow";
import { Banners, useLayout, Workbench } from "./shell/Workbench";
import type { AppStores } from "./state/app";
import { useMotion } from "./ui/motion";

/** Composition root: store subscriptions, app-wide effects, dialog state; the shell is in shell/. */
export function App({ app }: { app: AppStores }) {
  const { client, workspace, documents, settings } = app;
  const daemon = useSyncExternalStore((fn) => client.onState(fn), () => client.state);
  const ws = useSyncExternalStore(workspace.subscribe, workspace.getSnapshot);
  const docs = useSyncExternalStore(documents.subscribe, documents.getSnapshot);
  const task = useSyncExternalStore(app.tasks.subscribe, app.tasks.getSnapshot);
  const prefs = useSyncExternalStore(settings.subscribe, settings.getSnapshot);
  const systemDark = useSystemDark();
  const dark = prefs.theme === "dark" || (prefs.theme === "system" && systemDark);
  const layout = useLayout();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [palette, setPalette] = useState<"commands" | "files" | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const ready = daemon.state === "ready";

  // ---- theme, motion level & editor appearance -------------------------------
  useMotion(prefs.motion);
  useNativeTitleBarTheme(prefs.theme);
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

  const openFolder = useCallback(async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Open Folder" });
    if (typeof picked === "string") documents.requestOpenFolder(picked);
  }, [documents]);

  const ctx: ShellContext = {
    app, ready, dark, ws, docs, task, prefs, openFolder, setSettingsOpen, setAboutOpen, setPalette,
    hasDoc: docs.docs.length > 0 && docs.editorReady,
    toggleSidebar: layout.toggleSidebar, togglePanel: layout.togglePanel, resetLayout: layout.resetLayout, focusRun: layout.focusRun,
  };
  useNativeMenu(ctx);
  useAppShortcuts(ctx);
  const commands = usePaletteCommands(ctx, palette);

  return (
    <div className="shell">
      <TitleRow ctx={ctx} layout={layout.layout} />
      <Banners ctx={ctx} daemon={daemon} />
      <Workbench ctx={ctx} layout={layout} />
      <StatusBar ctx={ctx} daemon={daemon} />
      <ConflictDialog documents={documents} />
      <UnsavedDialog documents={documents} />
      <SettingsDialog settings={settings} open={settingsOpen || aboutOpen} section={aboutOpen ? "about" : "appearance"} info={info}
        onOpenChange={(o) => !o && (setSettingsOpen(false), setAboutOpen(false))} serverVersion={daemon.state === "ready" ? daemon.serverVersion : null}
        onRestartBackend={() => void client.restart()} onOpenExternal={(url) => void host.openExternal(url)} />
      <CommandPalette
        open={palette !== null}
        mode={palette ?? "commands"}
        onOpenChange={(o) => !o && setPalette(null)}
        commands={commands}
        hasWorkspace={!!ctx.ws.id}
        onOpenFolder={() => void ctx.openFolder()}
      />
    </div>
  );
}
