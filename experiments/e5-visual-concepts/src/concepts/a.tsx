// Concept A — Precision Workstation ("Graphite & Brass").
// Machined graphite surfaces joined by a dark seam + lit edge; one brass signal
// colour; title bar with a command field and a task transport; activity rail.
import { useRef } from "react";
import "./a.css";
import "./r.css";
import * as I from "../icons";
import { TASK } from "../data";
import {
  Activity, AgentView, Code, ComponentSheet, ConsoleTabs, Crumbs, EnvView, IBtn, Kbd, Led, MockBadge,
  Output, Palette, Panel, Prompt, SettingsView, Tabs, Toast, Tree, useHint, usePresence,
} from "../shared";
import { useResize, type Workbench } from "../workbench";

const LEVELS = [2, 3, 9, 14, 6, 3, 1, 4, 11, 13, 12, 7, 5, 8, 6, 4];

/** variant "r" is the recommended synthesis (A + B's instrument signals + C's tonal grammar). */
export function ConceptA({ wb, variant = "a" }: { wb: Workbench; variant?: "a" | "r" }) {
  const root = useRef<HTMLDivElement>(null);
  const hint = useHint(root, "Ready");
  const side = usePresence(wb.sidebar, 140);
  const panel = usePresence(wb.panel, 140);
  const dock = usePresence(wb.dock, 160);
  const dragSide = useResize(root, "--side-w", "x", false, wb.sideW, wb.setSideW, 180, 480);
  const dragPanel = useResize(root, "--panel-h", "y", true, wb.panelH, wb.setPanelH, 120, 520);
  const isWs = wb.view === "workspace";

  return (
    <div
      ref={root}
      className={`concept ca ${variant === "r" ? "cr" : ""}`}
      data-theme={wb.theme}
      style={{ ["--side-w" as string]: `${wb.sideW}px`, ["--panel-h" as string]: `${wb.panelH}px`, ["--dock-w" as string]: `${wb.dockW}px` }}
    >
      <header className="a-title">
        <div className="a-brand"><I.Mark className="a-mark" /><span>Axiom</span></div>
        <nav className="a-menus" aria-label="Application menu">
          {["File", "Edit", "View", "Run", "Help"].map((m) => <button key={m} type="button" className="a-menu" data-hint={`${m} menu — Alt+${m[0]}`}>{m}</button>)}
        </nav>
        <span className="a-sep" aria-hidden />
        <button type="button" className="a-ws" data-hint="Switch workspace — recent folders and Open Folder…">
          <span className="a-ws__name">axiom</span><span className="a-ws__path">~/src/axiom</span><I.ChevronDown width={12} height={12} />
        </button>
        <span className="a-spacer" />
        <button type="button" className="a-cmd" onClick={() => wb.setPalette("commands")} data-hint="Command palette — every action, with its shortcut">
          <I.Search width={14} height={14} /><span>Search files, run commands</span><Kbd k="Ctrl K" />
        </button>
        <span className="a-spacer" />
        <div className="a-transport" role="group" aria-label="Task">
          <Led state="run" label="Task running" />
          <span className="a-transport__label">{TASK.label}</span>
          <span className="a-transport__time">{TASK.elapsed}</span>
          <IBtn label="Rerun (F5)" hint="Rerun the last command — F5"><I.Rerun /></IBtn>
          <IBtn label="Stop (Shift+F5)" hint="Stop the running task and its child processes — Shift+F5"><I.Stop /></IBtn>
          {variant === "r" && (
            <span className="r-meter" role="img" aria-label="Output rate, last 4 seconds" data-hint="Output rate — lines per 250 ms over the last 4 s">
              {LEVELS.map((v, i) => <span key={i} style={{ ["--v" as string]: v / 14 }} />)}
            </span>
          )}
          <Activity running />
        </div>
        <div className="a-toggles">
          <IBtn label="Side bar (Ctrl+B)" pressed={wb.sidebar} onClick={() => wb.toggle("sidebar")}><I.Sidebar /></IBtn>
          <IBtn label="Console (Ctrl+J)" pressed={wb.panel} onClick={() => wb.toggle("panel")}><I.PanelBottom /></IBtn>
          <IBtn label="Agent dock (Ctrl+L)" pressed={wb.dock} onClick={() => wb.toggle("dock")}><I.PanelRight /></IBtn>
        </div>
      </header>

      <div className="a-body" data-side={side.mounted} data-dock={dock.mounted && isWs}>
        <nav className="a-rail" aria-label="Views">
          <RailBtn icon={<I.Files />} label="Files" active={isWs} onClick={() => wb.setView("workspace")} />
          <RailBtn icon={<I.Search />} label="Search" future="M4" />
          <RailBtn icon={<I.Layers />} label="Environments" future="M2" active={wb.view === "environments"} onClick={() => wb.setView("environments")} badge />
          <RailBtn icon={<I.Agent />} label="Agents" future="M3" onClick={() => wb.toggle("dock")} badge={false} dot />
          <RailBtn icon={<I.Crosshair />} label="Inspect" future="M5" />
          <span className="a-rail__spacer" />
          <RailBtn icon={<I.Gear />} label="Settings" active={wb.view === "settings"} onClick={() => wb.setView("settings")} />
        </nav>

        {side.mounted && (
          <>
            <Panel region="files" className="a-side" title="Files" meta="axiom" tools={<IBtn label="Collapse all"><I.Chevron style={{ transform: "rotate(90deg)" }} /></IBtn>}>
              <div className="motion-in-left" data-phase={side.phase}><Tree active={wb.active} onOpen={wb.setActive} /></div>
            </Panel>
            <div className="split split--v" role="separator" aria-orientation="vertical" aria-label="Resize side bar" tabIndex={0} onPointerDown={dragSide} data-hint="Drag to resize · double-click to reset" />
          </>
        )}

        <main className="a-main">
          {isWs && (
            <div className="a-editor">
              <Tabs active={wb.active} onActivate={wb.setActive} />
              <Crumbs />
              <Code active={wb.active} concept="a" theme={wb.theme} />
            </div>
          )}
          {wb.view === "environments" && <div className="a-page"><EnvView /></div>}
          {wb.view === "settings" && <div className="a-page"><SettingsView /></div>}
          {wb.view === "components" && <div className="a-page"><ComponentSheet /></div>}
          {panel.mounted && isWs && (
            <>
              <div className="split split--h" role="separator" aria-orientation="horizontal" aria-label="Resize console" tabIndex={0} onPointerDown={dragPanel} />
              <Panel
                region="console"
                className="a-console"
                title="Console"
                meta={<><ConsoleTabs value="Output" /><span className="a-sep" /><Led state="run" /><span className="mono">{TASK.label}</span><span className="faint mono">{TASK.elapsed}</span></>}
                tools={<><IBtn label="Copy all"><I.Copy /></IBtn><IBtn label="Clear"><I.Trash /></IBtn><IBtn label="Hide console (Ctrl+J)" onClick={() => wb.toggle("panel")}><I.Close /></IBtn></>}
              >
                <div className="motion-in-up" data-phase={panel.phase}>
                  <Output />
                  <Prompt />
                </div>
              </Panel>
            </>
          )}
        </main>

        {dock.mounted && isWs && (
          <Panel region="agent" className="a-dock" title="Agent" meta={<MockBadge m="M3" />} tools={<IBtn label="Session history"><I.More /></IBtn>}>
            <div className="motion-in-right" data-phase={dock.phase}><AgentView /></div>
          </Panel>
        )}
      </div>

      <footer className="statusbar" aria-label="Status">
        <span className="sb__seg" data-hint="Backend connection — click to restart axiomd"><Led state="on" label="Connected" /> axiomd 0.1.0</span>
        <span className="sb__seg sb__seg--task"><span className="spin" /> {TASK.label} · 7/14 passed</span>
        <span className="sb__hint" aria-live="off">{variant === "r" ? <HintText text={hint} /> : hint}</span>
        <span className="sb__seg">1 <I.Warn width={12} height={12} /></span>
        <span className="sb__seg">Ln 84, Col 17</span>
        <span className="sb__seg">Spaces: 4</span>
        <span className="sb__seg">UTF-8</span>
        <span className="sb__seg">LF</span>
        <span className="sb__seg">Rust</span>
      </footer>

      <Palette open={wb.palette !== null} mode={wb.palette ?? "commands"} onClose={() => wb.setPalette(null)} />
      <Toast open={wb.toast}><I.Check width={14} height={14} /> Saved <b>executor.rs</b><span className="faint"> · through axiomd</span></Toast>
    </div>
  );
}

function RailBtn({ icon, label, future, active, onClick, badge, dot }: { icon: React.ReactNode; label: string; future?: string; active?: boolean; onClick?: () => void; badge?: boolean; dot?: boolean }) {
  return (
    <button type="button" className="a-rail__btn" aria-label={future ? `${label} (${future})` : label} aria-current={active ? "page" : undefined} data-future={future && !onClick ? true : undefined} onClick={onClick} data-hint={future ? `${label} — arrives in ${future}` : label}>
      {icon}
      {badge && <span className="a-rail__badge">1</span>}
      {dot && <span className="a-rail__dot" />}
    </button>
  );
}

/** Status-bar hint (FL Studio's hint bar, adapted): "Action — Shortcut" renders the shortcut as keycaps. */
function HintText({ text }: { text: string }) {
  const [what, keys] = text.split(" — ");
  const isKeys = keys && /^(Ctrl|Shift|Alt|F\d|Enter|Esc|↑)/.test(keys);
  return (
    <>
      <span className="r-hint__what">{what}</span>
      {keys && (isKeys ? <span className="r-hint__keys">{keys.split(/[+ ,]+/).filter(Boolean).map((k) => <kbd key={k} className="kbd">{k}</kbd>)}</span> : <span className="r-hint__more"> — {keys}</span>)}
    </>
  );
}
