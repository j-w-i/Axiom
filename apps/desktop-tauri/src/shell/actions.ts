// The shell's actions: one implementation per action, shared by the native menu
// (`axiom://menu` events, T-07), the in-window menus, the shortcuts and the palette.

import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import type { AppStores } from "../state/app";
import type { Destination } from "./Rail";
import type { DocumentsState } from "../state/documents";
import type { SettingsState } from "../state/settings";
import type { TasksState } from "../state/tasks";
import type { WorkspaceState } from "../state/workspace";

/** Everything the shell modules read or call; built once per render by App. */
export interface ShellContext {
  app: AppStores;
  ready: boolean;
  hasDoc: boolean;
  dark: boolean;
  ws: WorkspaceState;
  docs: DocumentsState;
  task: TasksState;
  prefs: SettingsState;
  openFolder: () => Promise<void>;
  toggleSidebar: () => void;
  togglePanel: () => void;
  resetLayout: () => void;
  focusRun: () => void;
  setSettingsOpen: (open: boolean) => void;
  setAboutOpen: (open: boolean) => void;
  setPalette: (palette: "commands" | "files" | null) => void;
  /** The rail's current destination (M2-UI-001). */
  destination: Destination;
  setDestination: (d: Destination) => void;
}

/** The ids the native menu sends (T-07); unknown ids are ignored. */
export function runMenuAction(id: string, ctx: ShellContext): void {
  const { documents, tasks } = ctx.app;
  switch (id) {
    case "open-folder": if (ctx.ready) void ctx.openFolder(); break;
    case "save": if (ctx.hasDoc) documents.save(ctx.docs.activePath); break;
    case "save-all": if (ctx.hasDoc) documents.saveAll(); break;
    case "close-folder": if (ctx.ws.id) documents.requestCloseFolder(); break;
    case "settings": ctx.setSettingsOpen(true); break;
    case "find": if (ctx.hasDoc) documents.runCommand("find"); break;
    case "toggle-sidebar": ctx.toggleSidebar(); break;
    case "toggle-panel": ctx.togglePanel(); break;
    case "run-command": ctx.focusRun(); break;
    case "rerun": if (tasks.canRun) tasks.rerunLast(); break;
    case "stop": tasks.stop(); break;
    case "about": ctx.setAboutOpen(true); break;
    default: break;
  }
}

/** Native menu (macOS): the menu bar dispatches the same actions, as `axiom://menu` events. */
export function useNativeMenu(ctx: ShellContext): void {
  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void listen<string>("axiom://menu", (event) => runMenuAction(event.payload, ctx))
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
  }, [ctx.ready, ctx.hasDoc, ctx.ws.id, ctx.docs.activePath, ctx.dark, ctx.prefs.theme]);
}
