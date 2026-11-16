// Concept C — Modern Technical Studio ("Ink & Cinnabar").
// Tiles float on an ink canvas with even gutters; a page bar at the bottom
// switches whole workspaces (Code / Agents / Environments / Inspect); labels
// are monospaced technical captions; syntax colour is deliberately sparse.
import { useRef } from "react";
import "./c.css";
import * as I from "../icons";
import { TASK } from "../data";
import {
  AgentView, Code, ComponentSheet, ConsoleTabs, Crumbs, EnvView, IBtn, Kbd, Led, MockBadge, Output, Palette,
  Panel, Prompt, SettingsView, Tabs, Toast, Tree, useHint, usePresence,
} from "../shared";
import { useResize, type Workbench, type View } from "../workbench";

export function ConceptC({ wb }: { wb: Workbench }) {
  const root = useRef<HTMLDivElement>(null);
  const hint = useHint(root, "");
  const side = usePresence(wb.sidebar, 160);
  const panel = usePresence(wb.panel, 160);
  const dock = usePresence(wb.dock, 180);
  const dragSide = useResize(root, "--side-w", "x", false, wb.sideW, wb.setSideW, 180, 480);
  const dragPanel = useResize(root, "--panel-h", "y", true, wb.panelH, wb.setPanelH, 120, 520);
  const isWs = wb.view === "workspace";
  const pages: [string, View | null, string?][] = [["Code", "workspace"], ["Agents", "workspace", "M3"], ["Environments", "environments", "M2"], ["Inspect", null, "M5"]];

  return (
    <div ref={root} className="concept cc" data-theme={wb.theme} style={{ ["--side-w" as string]: `${wb.sideW}px`, ["--panel-h" as string]: `${wb.panelH}px`, ["--dock-w" as string]: `${wb.dockW}px` }}>
      <header className="c-top">
        <IBtn label="Menu (Alt)" hint="Application menu — Alt"><I.Menu /></IBtn>
        <I.Mark className="c-mark" />
        <span className="c-ws"><b>axiom</b><span className="c-ws__path">~/src/axiom</span></span>
        <span className="c-spacer" />
        <button type="button" className="c-cmd" onClick={() => wb.setPalette("commands")} data-hint="Command palette — Ctrl+K">
          <I.Search width={14} height={14} /><span>Go to file, run a command, ask the agent</span><Kbd k="Ctrl K" />
        </button>
        <span className="c-spacer" />
        <div className="c-task" role="group" aria-label="Task">
          <Led state="run" label="Task running" />
          <span className="c-task__label">{TASK.label}</span>
          <span className="c-task__time">{TASK.elapsed}</span>
          <button type="button" className="c-task__stop" aria-label="Stop (Shift+F5)" data-hint="Stop — Shift+F5"><I.Stop width={11} height={11} /></button>
        </div>
        <IBtn label="Files (Ctrl+B)" pressed={wb.sidebar} onClick={() => wb.toggle("sidebar")}><I.Sidebar /></IBtn>
        <IBtn label="Console (Ctrl+J)" pressed={wb.panel} onClick={() => wb.toggle("panel")}><I.PanelBottom /></IBtn>
        <IBtn label="Agent (Ctrl+L)" pressed={wb.dock} onClick={() => wb.toggle("dock")}><I.PanelRight /></IBtn>
      </header>

      <div className="c-canvas">
        {side.mounted && isWs && (
          <>
            <Panel region="files" className="c-tile c-side" index="01" title="Files" meta={<span className="c-count">33</span>} tools={<IBtn label="More"><I.More /></IBtn>}>
              <div className="motion-in-left" data-phase={side.phase}><Tree active={wb.active} onOpen={wb.setActive} /></div>
            </Panel>
            <div className="split split--v" role="separator" aria-orientation="vertical" aria-label="Resize files" tabIndex={0} onPointerDown={dragSide} />
          </>
        )}
        <div className="c-center">
          {isWs ? (
            <section className="c-tile c-editor" aria-label="Editor">
              <Tabs active={wb.active} onActivate={wb.setActive} numbered />
              <Crumbs />
              <Code active={wb.active} concept="c" theme={wb.theme} />
            </section>
          ) : (
            <section className="c-tile c-page">
              {wb.view === "environments" && <EnvView />}
              {wb.view === "settings" && <SettingsView />}
              {wb.view === "components" && <ComponentSheet />}
            </section>
          )}
          {panel.mounted && isWs && (
            <>
              <div className="split split--h" role="separator" aria-orientation="horizontal" aria-label="Resize console" tabIndex={0} onPointerDown={dragPanel} />
              <Panel region="console" className="c-tile c-console" index="04" title="Console" meta={<ConsoleTabs value="Output" />} tools={<><IBtn label="Copy all"><I.Copy /></IBtn><IBtn label="Clear"><I.Trash /></IBtn></>}>
                <div className="motion-in-up" data-phase={panel.phase}><Output /><Prompt /></div>
              </Panel>
            </>
          )}
        </div>
        {dock.mounted && isWs && (
          <>
            <div className="split split--v" aria-hidden />
            <Panel region="agent" className="c-tile c-dock" index="03" title="Agent" meta={<MockBadge m="M3" />} tools={<IBtn label="More"><I.More /></IBtn>}>
              <div className="motion-in-right" data-phase={dock.phase}><AgentView /></div>
            </Panel>
          </>
        )}
      </div>

      <footer className="c-pagebar statusbar" aria-label="Pages and status">
        <nav className="c-pages" aria-label="Pages">
          {pages.map(([label, v, m]) => (
            <button key={label} type="button" className="c-page-btn" aria-current={(v === wb.view && (label === "Code" ? true : label !== "Agents")) ? "page" : undefined} data-future={v === null || undefined} onClick={() => v && wb.setView(v)} data-hint={m ? `${label} — arrives in ${m}` : label}>
              {label}{m && <span className="c-page-btn__m">{m}</span>}
            </button>
          ))}
        </nav>
        <span className="sb__hint">{hint}</span>
        <span className="sb__seg"><Led state="on" label="Connected" /> axiomd 0.1.0</span>
        <span className="sb__seg">7/14</span>
        <span className="sb__seg">84:17</span>
        <span className="sb__seg">utf-8 · lf</span>
        <span className="sb__seg">rust</span>
        <button type="button" className="ibtn" aria-label="Settings" onClick={() => wb.setView(wb.view === "settings" ? "workspace" : "settings")}><I.Gear /></button>
      </footer>

      <Palette open={wb.palette !== null} mode={wb.palette ?? "commands"} onClose={() => wb.setPalette(null)} />
      <Toast open={wb.toast}><I.Check width={14} height={14} /> Saved executor.rs</Toast>
    </div>
  );
}
