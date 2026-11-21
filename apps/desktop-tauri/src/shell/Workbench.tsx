// The workbench: side bar, splitters, main column (editor or Welcome) and the
// console, with the per-machine layout state; and the banner strip above it.

import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties, type FocusEvent } from "react";
import { ChatPage, ChatSide } from "../components/Chat";
import { ModelSetup } from "../components/Models";
import { shellStorage } from "../state/shell-storage";
import { EditorArea } from "../components/EditorArea";
import { attentionCount, EnvironmentPage, EnvironmentsSide } from "../components/Environments";
import { Explorer } from "../components/Explorer";
import { OutputPanel, type OutputPanelHandle } from "../components/OutputPanel";
import { Welcome } from "../components/Welcome";
import type { DaemonState } from "../daemon/types";
import * as Icon from "../ui/icons";
import { usePresence } from "../ui/motion";
import { Panel, Splitter } from "../ui/panel";
import { Banner } from "../ui/overlays";
import { IconButton } from "../ui/primitives";
import type { ShellContext } from "./actions";
import { clampLayout, defaultLayout, LayoutStore, LIMITS, maxSizes } from "./layout";
import { MOD } from "./platform";
import { Rail } from "./Rail";
import { focusEditor } from "./shortcuts";

/** A region's exit lasts --dur-1 (90 ms) in Standard; Presence makes it 0 in Reduced and Off. */
export const REGION_EXIT_MS = 90;

/** True when keyboard focus is inside the region `name` (data-region). */
const focusIn = (name: string) => !!document.activeElement?.closest(`[data-region="${name}"]`);

const windowSize = () => ({ width: window.innerWidth, height: window.innerHeight });

/** The layout store (axiom.layout.v2, shell/layout.ts) and the actions that change it.
 * The caller re-renders only when a visibility flag changes; sizes are the workbench's. */
export function useLayout() {
  const store = useRef<LayoutStore | null>(null);
  store.current ??= new LayoutStore(shellStorage, windowSize());
  const s = store.current;
  const sidebarVisible = useSyncExternalStore(s.subscribe, () => s.get().sidebarVisible);
  const panelVisible = useSyncExternalStore(s.subscribe, () => s.get().panelVisible);
  const dockVisible = useSyncExternalStore(s.subscribe, () => s.get().dockVisible);
  const panelRef = useRef<OutputPanelHandle>(null);
  // Hiding a region that holds focus gives it to the editor first, on the input frame
  // (motion-system.md §3 rule 3), before the exit animation starts.
  const toggleSidebar = () => {
    if (s.get().sidebarVisible && focusIn("files")) focusEditor();
    s.update((l) => ({ ...l, sidebarVisible: !l.sidebarVisible }));
  };
  const togglePanel = () => {
    if (s.get().panelVisible && focusIn("console")) focusEditor();
    s.update((l) => ({ ...l, panelVisible: !l.panelVisible }));
  };
  const toggleDock = () => {
    if (s.get().dockVisible && focusIn("dock")) focusEditor();
    s.update((l) => ({ ...l, dockVisible: !l.dockVisible }));
  };
  const focusRun = () => {
    s.update((l) => (l.panelVisible ? l : { ...l, panelVisible: true }));
    setTimeout(() => panelRef.current?.focusCommand(), 0);
  };
  const resetLayout = () => s.update(() => defaultLayout(windowSize()));
  return { store: s, layout: { sidebarVisible, panelVisible, dockVisible }, panelRef, toggleSidebar, togglePanel, toggleDock, focusRun, resetLayout };
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
  const { workspace, documents, tasks, environments } = ctx.app;
  const { ws, ready, destination } = ctx;
  const envPage = destination === "environments";
  const chatPage = destination === "chat";
  const page = envPage || chatPage;
  const root = useRef<HTMLDivElement>(null);
  const dirtyPaths = useMemo(() => new Set(ctx.docs.docs.filter((d) => d.dirty).map((d) => d.path)), [ctx.docs.docs]);
  const layout = clampLayout(useSyncExternalStore(store.subscribe, store.get), windowSize());
  const max = maxSizes(windowSize());
  const style = { "--side-w": `${layout.sidebar}px`, "--panel-h": `${layout.panel}px`, "--dock-w": `${layout.dock}px` } as CSSProperties;
  // The focused region is DOM state (no React render per focus change).
  const onFocus = (e: FocusEvent<HTMLDivElement>) => {
    const region = regionOf(e.target);
    if (region && root.current) root.current.dataset.focusedRegion = region;
  };
  // Regions stay mounted through their exit (one commit removes the column afterwards); only
  // their content animates (transform + opacity). No entrance on the first render.
  const side = useRegionPresence(layout.sidebarVisible);
  const panel = useRegionPresence(layout.panelVisible);
  // The dock works in a folder, beside the files (not in Chat, which is the same conversation full-window).
  const dockShown = layout.dockVisible && !!ws.id && !chatPage;
  const dock = useRegionPresence(dockShown);
  const agent = ctx.app.agent;
  useEffect(() => {
    if (dockShown) agent.setScope("workspace");
  }, [dockShown, agent]);
  const onBlur = (e: FocusEvent<HTMLDivElement>) => {
    if (!root.current?.contains(e.relatedTarget as Node | null)) delete root.current?.dataset.focusedRegion;
  };
  return (
    <div className="workbench" ref={root} style={style} onFocus={onFocus} onBlur={onBlur}>
      <Rail
        current={destination}
        onSelect={ctx.setDestination}
        attention={ws.id ? () => attentionCount(environments) : () => 0}
        subscribe={environments.subscribe}
        chatAttention={() => agent.getSnapshot().approvals.length}
        chatSubscribe={agent.subscribe}
      />
      {side.mounted && (
        <>
          <Panel
            className="sidebar motion-in-left"
            title={envPage ? "Environments" : chatPage ? "Chat" : "Files"}
            region="files"
            phase={side.phase}
            meta={chatPage ? undefined : ws.name || undefined}
            tools={
              chatPage ? (
                <IconButton label="New chat" shortcut="Mod+N" onClick={() => ctx.app.agent.newChat()}>
                  <Icon.Plus />
                </IconButton>
              ) : ws.id ? (
                envPage ? (
                  <IconButton label="Detect toolchains again" onClick={() => void environments.refresh()}>
                    <Icon.Refresh />
                  </IconButton>
                ) : (
                  <IconButton label="Refresh explorer" onClick={() => workspace.applyChanges([""])}>
                    <Icon.Refresh />
                  </IconButton>
                )
              ) : undefined
            }
          >
            {/* The explorer stays mounted (expansion, scroll and its active row survive). */}
            <div className="sidebar__view" hidden={page}>
              <Explorer workspace={workspace} onOpenFile={(p) => documents.open(p)} dirty={dirtyPaths} onReopen={() => documents.requestOpenFolder(ws.root)} />
            </div>
            {envPage && <EnvironmentsSide store={environments} />}
            {chatPage && <ChatSide store={ctx.app.agent} workspaceName={ws.id ? ws.name : ""} />}
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
          {/* The editor stays mounted (Monaco keeps its state) while the Environments page shows. */}
          <div className="editor-column__files" hidden={page}>
            {ws.id || ws.opening ? (
              <EditorArea documents={documents} lsp={ctx.app.lsp} mod={MOD} />
            ) : (
              <Welcome recent={ws.recent} ready={ready} onOpenFolder={() => void ctx.openFolder()} onOpenRecent={(r) => documents.requestOpenFolder(r)} onStartChat={() => ctx.setDestination("chat")} mod={MOD} />
            )}
          </div>
          {envPage && <EnvironmentPage store={environments} hasWorkspace={!!ws.id} onOpenFolder={() => void ctx.openFolder()} />}
          {chatPage && (
            <ChatPage store={ctx.app.agent} hasWorkspace={!!ws.id} dark={ctx.dark} onOpenLink={(url) => ctx.app.documents.openExternal(url)} setup={<ModelSetup store={ctx.app.agent} hasWorkspace={!!ws.id} />} />
          )}
        </div>
        {/* Chat is a full-window surface: no console (its visibility setting is untouched). */}
        {panel.mounted && !chatPage && (
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
              <div className="console-motion motion-in-up" data-phase={panel.phase}>
                <OutputPanel
                  ref={panelRef}
                  tasks={tasks}
                  workspace={workspace}
                  problems={ctx.app.problems}
                  ready={ready}
                  onHide={ctx.togglePanel}
                  onOpenProblem={(d) => d.location && documents.openAt(d.location.file, d.location.line, d.location.column ?? 1)}
                />
              </div>
            </div>
          </>
        )}
      </main>
      {dock.mounted && (
        <>
          <Splitter
            orientation="vertical"
            label="Resize AI dock"
            value={layout.dock}
            min={LIMITS.dock.min}
            max={max.dock}
            defaultValue={LIMITS.dock.def}
            invert
            target={root}
            cssVar="--dock-w"
            onCommit={(d) => store.update((l) => ({ ...l, dock: d }))}
          />
          <aside className="dock motion-in-right" data-region="dock" data-phase={dock.phase} aria-label="AI dock">
            <ChatPage store={agent} hasWorkspace dark={ctx.dark} variant="dock" onOpenLink={(url) => ctx.app.documents.openExternal(url)} setup={<ModelSetup store={agent} hasWorkspace />} />
          </aside>
        </>
      )}
    </div>
  );
}

/** usePresence for a region, without a phase (no entrance animation) until it first toggles. */
function useRegionPresence(visible: boolean): { mounted: boolean; phase: "in" | "out" | undefined } {
  const p = usePresence(visible, REGION_EXIT_MS);
  const [initial] = useState(visible);
  const toggled = useRef(false);
  if (visible !== initial) toggled.current = true;
  return { mounted: p.mounted, phase: toggled.current ? p.phase : undefined };
}
