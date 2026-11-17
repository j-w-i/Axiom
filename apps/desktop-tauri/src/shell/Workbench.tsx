// The workbench: side bar, splitters, main column (editor or Welcome) and the
// console, with the per-machine layout state; and the banner strip above it.

import { useEffect, useRef, useState } from "react";
import { EditorArea } from "../components/EditorArea";
import { Explorer } from "../components/Explorer";
import { OutputPanel, type OutputPanelHandle } from "../components/OutputPanel";
import { Welcome } from "../components/Welcome";
import type { DaemonState } from "../daemon/types";
import * as Icon from "../ui/icons";
import { Banner, IconButton, Splitter } from "../ui/primitives";
import type { ShellContext } from "./actions";
import { MOD } from "./platform";

/** Shell-local layout (per machine, like the Qt shell's window QSettings). */
export interface Layout { sidebar: number; panel: number; sidebarVisible: boolean; panelVisible: boolean }
export const LAYOUT_KEY = "axiom.layout.v1";
export function loadLayout(): Layout {
  const d: Layout = { sidebar: 260, panel: 220, sidebarVisible: window.innerWidth >= 900, panelVisible: true };
  try {
    return { ...d, ...(JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? "{}") as Partial<Layout>) };
  } catch {
    return d;
  }
}

/** The layout state and the actions that change it. */
export function useLayout() {
  const [layout, setLayout] = useState<Layout>(loadLayout);
  useEffect(() => localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout)), [layout]);
  const panelRef = useRef<OutputPanelHandle>(null);
  const toggleSidebar = () => setLayout((l) => ({ ...l, sidebarVisible: !l.sidebarVisible }));
  const togglePanel = () => setLayout((l) => ({ ...l, panelVisible: !l.panelVisible }));
  const focusRun = () => {
    setLayout((l) => ({ ...l, panelVisible: true }));
    setTimeout(() => panelRef.current?.focusCommand(), 0);
  };
  return { layout, setLayout, panelRef, toggleSidebar, togglePanel, focusRun };
}

export type LayoutHandle = ReturnType<typeof useLayout>;

export function Banners({ ctx, daemon }: { ctx: ShellContext; daemon: DaemonState }) {
  const { client, documents, workspace, settings } = ctx.app;
  const { docs, ws, prefs } = ctx;
  return (
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
  );
}

export function Workbench({ ctx, layout: { layout, setLayout, panelRef } }: { ctx: ShellContext; layout: LayoutHandle }) {
  const { workspace, documents, tasks } = ctx.app;
  const { ws, ready } = ctx;
  return (
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
            <Welcome recent={ws.recent} ready={ready} onOpenFolder={() => void ctx.openFolder()} onOpenRecent={(r) => documents.requestOpenFolder(r)} mod={MOD} />
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
  );
}
