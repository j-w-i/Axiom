// The workbench: side bar, splitters, main column (editor or Welcome) and the
// console, with the per-machine layout state; and the banner strip above it.

import { useRef, useSyncExternalStore, type CSSProperties, type FocusEvent } from "react";
import { EditorArea } from "../components/EditorArea";
import { Explorer } from "../components/Explorer";
import { OutputPanel, type OutputPanelHandle } from "../components/OutputPanel";
import { Welcome } from "../components/Welcome";
import type { DaemonState } from "../daemon/types";
import * as Icon from "../ui/icons";
import { Panel, Splitter } from "../ui/panel";
import { Banner, IconButton } from "../ui/primitives";
import type { ShellContext } from "./actions";
import { clampLayout, defaultLayout, LayoutStore, LIMITS, maxSizes } from "./layout";
import { MOD } from "./platform";

const windowSize = () => ({ width: window.innerWidth, height: window.innerHeight });

/** The layout store (axiom.layout.v2, shell/layout.ts) and the actions that change it.
 * The caller re-renders only when a visibility flag changes; sizes are the workbench's. */
export function useLayout() {
  const store = useRef<LayoutStore | null>(null);
  store.current ??= new LayoutStore(localStorage, windowSize());
  const s = store.current;
  const sidebarVisible = useSyncExternalStore(s.subscribe, () => s.get().sidebarVisible);
  const panelVisible = useSyncExternalStore(s.subscribe, () => s.get().panelVisible);
  const panelRef = useRef<OutputPanelHandle>(null);
  const toggleSidebar = () => s.update((l) => ({ ...l, sidebarVisible: !l.sidebarVisible }));
  const togglePanel = () => s.update((l) => ({ ...l, panelVisible: !l.panelVisible }));
  const focusRun = () => {
    s.update((l) => (l.panelVisible ? l : { ...l, panelVisible: true }));
    setTimeout(() => panelRef.current?.focusCommand(), 0);
  };
  const resetLayout = () => s.update(() => defaultLayout(windowSize()));
  return { store: s, layout: { sidebarVisible, panelVisible }, panelRef, toggleSidebar, togglePanel, focusRun, resetLayout };
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

/** Which region an element belongs to (data-region of its nearest region container). */
const regionOf = (el: EventTarget | null): string | undefined =>
  (el instanceof Element ? el.closest<HTMLElement>("[data-region]")?.dataset.region : undefined) ?? undefined;

export function Workbench({ ctx, layout: { store, panelRef } }: { ctx: ShellContext; layout: LayoutHandle }) {
  const { workspace, documents, tasks } = ctx.app;
  const { ws, ready } = ctx;
  const root = useRef<HTMLDivElement>(null);
  const layout = clampLayout(useSyncExternalStore(store.subscribe, store.get), windowSize());
  const max = maxSizes(windowSize());
  const style = { "--side-w": `${layout.sidebar}px`, "--panel-h": `${layout.panel}px` } as CSSProperties;
  // The focused region is DOM state (no React render per focus change).
  const onFocus = (e: FocusEvent<HTMLDivElement>) => {
    const region = regionOf(e.target);
    if (region && root.current) root.current.dataset.focusedRegion = region;
  };
  const onBlur = (e: FocusEvent<HTMLDivElement>) => {
    if (!root.current?.contains(e.relatedTarget as Node | null)) delete root.current?.dataset.focusedRegion;
  };
  return (
    <div className="workbench" ref={root} style={style} onFocus={onFocus} onBlur={onBlur}>
      {layout.sidebarVisible && (
        <>
          <Panel
            className="sidebar"
            title="Files"
            region="files"
            meta={ws.name || undefined}
            tools={
              ws.id ? (
                <IconButton label="Refresh explorer" onClick={() => workspace.applyChanges([""])}>
                  <Icon.Refresh />
                </IconButton>
              ) : undefined
            }
          >
            <Explorer workspace={workspace} onOpenFile={(p) => documents.open(p)} />
          </Panel>
          <Splitter
            orientation="vertical"
            label="Resize side bar"
            value={layout.sidebar}
            min={LIMITS.sidebar.min}
            max={max.sidebar}
            defaultValue={LIMITS.sidebar.def}
            target={root}
            cssVar="--side-w"
            onCommit={(sidebar) => store.update((l) => ({ ...l, sidebar }))}
          />
        </>
      )}
      <main className="main">
        <div className="editor-column" data-region="editor">
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
              value={layout.panel}
              min={LIMITS.panel.min}
              max={max.panel}
              defaultValue={LIMITS.panel.def}
              invert
              target={root}
              cssVar="--panel-h"
              onCommit={(panel) => store.update((l) => ({ ...l, panel }))}
            />
            <div className="console-host" data-region="console">
              <OutputPanel ref={panelRef} tasks={tasks} workspace={workspace} ready={ready} />
            </div>
          </>
        )}
      </main>
    </div>
  );
}
