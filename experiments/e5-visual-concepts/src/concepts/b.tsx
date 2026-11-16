// Concept B — Creative Instrument ("Slate & Signal").
// A transport bar of recessed LCD modules (workspace, task, agent, hint);
// panels are rack modules with a colour identity strip, LED and condensed caps
// label; toggles are LED buttons; the console carries an output-rate meter.
import { useRef } from "react";
import "./b.css";
import * as I from "../icons";
import { TASK } from "../data";
import {
  AgentView, Code, ComponentSheet, ConsoleTabs, Crumbs, EnvView, IBtn, Kbd, Led, MockBadge, Output, Palette,
  Panel, Prompt, SettingsView, Tabs, Toast, Tree, useHint, usePresence,
} from "../shared";
import { useResize, type Workbench } from "../workbench";

// Output lines per 250 ms over the last 4 s (computed in the UI from task/output events).
const LEVELS = [2, 3, 9, 14, 6, 3, 1, 4, 11, 13, 12, 7, 5, 8, 6, 4];

export function ConceptB({ wb }: { wb: Workbench }) {
  const root = useRef<HTMLDivElement>(null);
  const hint = useHint(root, "Hover any control for its action and shortcut");
  const side = usePresence(wb.sidebar, 140);
  const panel = usePresence(wb.panel, 140);
  const dock = usePresence(wb.dock, 170);
  const dragSide = useResize(root, "--side-w", "x", false, wb.sideW, wb.setSideW, 180, 480);
  const dragPanel = useResize(root, "--panel-h", "y", true, wb.panelH, wb.setPanelH, 120, 520);
  const isWs = wb.view === "workspace";

  return (
    <div ref={root} className="concept cb" data-theme={wb.theme} style={{ ["--side-w" as string]: `${wb.sideW}px`, ["--panel-h" as string]: `${wb.panelH}px`, ["--dock-w" as string]: `${wb.dockW}px` }}>
      <header className="b-menubar">
        <I.Mark className="b-mark" />
        {["File", "Edit", "View", "Run", "Help"].map((m) => <button key={m} type="button" className="b-menu" data-hint={`${m} menu — Alt+${m[0]}`}>{m}</button>)}
        <span className="b-menubar__title">axiom — Axiom</span>
      </header>

      <div className="b-transport" role="toolbar" aria-label="Transport">
        <button type="button" className="b-mod b-ws" data-hint="Workspace — switch folder or open another">
          <span className="b-lcd b-lcd--ws"><b>AXIOM</b><small>~/src/axiom</small></span>
          <I.ChevronDown width={12} height={12} />
        </button>
        <div className="b-mod b-run" role="group" aria-label="Task">
          <button type="button" className="b-key b-key--play" aria-label="Rerun (F5)" data-hint="Rerun the last command — F5"><I.Play /></button>
          <button type="button" className="b-key" aria-label="Stop (Shift+F5)" data-hint="Stop the task and its child processes — Shift+F5"><I.Stop /></button>
          <span className="b-lcd b-lcd--task">
            <span className="b-lcd__row"><Led state="run" /><span className="b-lcd__label">{TASK.label}</span></span>
            <span className="b-lcd__time">{TASK.elapsed}</span>
          </span>
          <span className="b-meter" role="img" aria-label="Output rate, last 4 seconds" data-hint="Output rate — lines per 250 ms over the last 4 s">
            {LEVELS.map((v, i) => <span key={i} style={{ ["--v" as string]: v / 14 }} />)}
          </span>
        </div>
        <button type="button" className="b-mod b-agentmod" onClick={() => wb.toggle("dock")} data-hint="Agent sessions — 1 waiting for your approval (Ctrl+L)">
          <Led state="warn" />
          <span className="b-lcd b-lcd--agent"><b>AGENT</b><small>2 sessions · 1 waiting</small></span>
        </button>
        <div className="b-mod b-hint" aria-live="off">
          <span className="b-lcd b-lcd--hint"><span className="b-lcd__tag">HINT</span>{hint}</span>
        </div>
        <button type="button" className="b-mod b-cmd" onClick={() => wb.setPalette("commands")} data-hint="Command palette — Ctrl+K">
          <I.Search width={14} height={14} /><Kbd k="Ctrl K" />
        </button>
        <div className="b-mod b-toggles" role="group" aria-label="Layout">
          <LedToggle on={wb.sidebar} label="Files" k="Ctrl+B" onClick={() => wb.toggle("sidebar")} />
          <LedToggle on={wb.panel} label="Console" k="Ctrl+J" onClick={() => wb.toggle("panel")} />
          <LedToggle on={wb.dock} label="Agent" k="Ctrl+L" onClick={() => wb.toggle("dock")} />
        </div>
      </div>

      <div className="b-rack">
        {side.mounted && (
          <>
            <Panel region="files" className="b-side b-module" title="Browser" led="on"
              meta={<span className="b-headtabs" role="tablist"><button role="tab" aria-selected="true" type="button">Files</button><button role="tab" type="button" data-future data-hint="Search — arrives in M4">Search</button><button role="tab" type="button" aria-selected={wb.view === "environments"} onClick={() => wb.setView(wb.view === "environments" ? "workspace" : "environments")} data-hint="Environments — arrives in M2">Env</button></span>}
              tools={<IBtn label="Settings" onClick={() => wb.setView(wb.view === "settings" ? "workspace" : "settings")}><I.Gear /></IBtn>}>
              <div className="motion-in-left" data-phase={side.phase}><Tree active={wb.active} onOpen={wb.setActive} tags /></div>
            </Panel>
            <div className="split split--v" role="separator" aria-orientation="vertical" aria-label="Resize browser" tabIndex={0} onPointerDown={dragSide} />
          </>
        )}
        <div className="b-center">
          {isWs ? (
            <section className="b-module b-editor" data-region="editor" aria-label="Editor">
              <Tabs active={wb.active} onActivate={wb.setActive} />
              <Crumbs />
              <div className="b-well"><Code active={wb.active} concept="b" theme={wb.theme} /></div>
            </section>
          ) : (
            <section className="b-module b-page" data-region={wb.view === "environments" ? "env" : "editor"}>
              {wb.view === "environments" && <EnvView />}
              {wb.view === "settings" && <SettingsView />}
              {wb.view === "components" && <ComponentSheet />}
            </section>
          )}
          {panel.mounted && isWs && (
            <>
              <div className="split split--h" role="separator" aria-orientation="horizontal" aria-label="Resize console" tabIndex={0} onPointerDown={dragPanel} />
              <Panel region="console" className="b-console b-module" title="Console" led="run"
                meta={<ConsoleTabs value="Output" />}
                tools={<><IBtn label="Copy all"><I.Copy /></IBtn><IBtn label="Clear"><I.Trash /></IBtn></>}>
                <div className="motion-in-up" data-phase={panel.phase}><Output /><Prompt /></div>
              </Panel>
            </>
          )}
        </div>
        {dock.mounted && isWs && (
          <Panel region="agent" className="b-dock b-module" title="Agent" led="warn" meta={<MockBadge m="M3" />} tools={<IBtn label="Session history"><I.More /></IBtn>}>
            <div className="motion-in-right" data-phase={dock.phase}><AgentView /></div>
          </Panel>
        )}
      </div>

      <footer className="statusbar" aria-label="Status">
        <span className="sb__seg"><Led state="on" label="Connected" /> AXIOMD 0.1.0</span>
        <span className="sb__seg">7/14 PASSED</span>
        <span className="sb__hint" />
        <span className="sb__seg">1 <I.Warn width={12} height={12} /></span>
        <span className="sb__seg">LN 84 · COL 17</span>
        <span className="sb__seg">SPACES 4</span>
        <span className="sb__seg">UTF-8 · LF</span>
        <span className="sb__seg sb__seg--lang">RUST</span>
      </footer>

      <Palette open={wb.palette !== null} mode={wb.palette ?? "commands"} onClose={() => wb.setPalette(null)} />
      <Toast open={wb.toast}><Led state="on" /> Saved <b>executor.rs</b></Toast>
    </div>
  );
}

function LedToggle({ on, label, k, onClick }: { on: boolean; label: string; k: string; onClick: () => void }) {
  return (
    <button type="button" className="b-ledbtn" aria-pressed={on} onClick={onClick} data-hint={`Show or hide ${label} — ${k}`}>
      <span className="b-ledbtn__led" aria-hidden />
      {label}
    </button>
  );
}
