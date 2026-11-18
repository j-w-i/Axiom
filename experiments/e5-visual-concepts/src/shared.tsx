// Components shared by the three concepts. Markup and behaviour are common;
// each concept's stylesheet gives them their material. Structural differences
// (title bar vs transport bar vs page bar, rails, tiling) live in concepts/*.tsx.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import * as I from "./icons";
import {
  AGENT_SESSIONS, AGENT_STEPS, COMMANDS, CRUMBS, DIAGNOSTICS, DIFF, DOCS, ENVIRONMENTS, FILE_RESULTS,
  OUTPUT, TASK, TREE, ACTIVE_FILE, type Node,
} from "./data";
import { setupMonaco, themeFrom } from "./monaco";

// ---------------------------------------------------------------------------
// Motion helpers
// ---------------------------------------------------------------------------

/** Keeps a component mounted for `ms` after `open` turns false so it can play
 * an exit transition. Phase drives a data attribute the CSS keys off. */
export function usePresence(open: boolean, ms: number): { mounted: boolean; phase: "in" | "out" } {
  const [mounted, setMounted] = useState(open);
  useEffect(() => {
    if (open) {
      setMounted(true);
      return;
    }
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const id = setTimeout(() => setMounted(false), reduce ? 0 : ms);
    return () => clearTimeout(id);
  }, [open, ms]);
  return { mounted: mounted || open, phase: open ? "in" : "out" };
}

/** FL Studio's hint bar, adapted: whatever control is under the pointer (or
 * focused) describes itself in the status bar. Controls opt in with data-hint. */
export function useHint(root: React.RefObject<HTMLElement | null>, idle: string): string {
  const [hint, setHint] = useState(idle);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const pick = (t: EventTarget | null) => {
      const h = (t as HTMLElement | null)?.closest?.("[data-hint]")?.getAttribute("data-hint");
      setHint(h ?? idle);
    };
    const over = (e: Event) => pick(e.target);
    el.addEventListener("pointerover", over);
    el.addEventListener("focusin", over);
    return () => {
      el.removeEventListener("pointerover", over);
      el.removeEventListener("focusin", over);
    };
  }, [root, idle]);
  return hint;
}

// ---------------------------------------------------------------------------
// Small primitives
// ---------------------------------------------------------------------------

export function Led({ state, label }: { state: "on" | "run" | "warn" | "err" | "off"; label?: string }) {
  return <span className="led" data-state={state} role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true} />;
}

export function Kbd({ k }: { k: string }) {
  return (
    <span className="kbds">
      {k.split(" ").map((p) => (
        <kbd className="kbd" key={p}>{p}</kbd>
      ))}
    </span>
  );
}

export function MockBadge({ m }: { m: string }) {
  return <span className="badge badge--mock" title={`Mock data: this surface arrives in ${m}`}>Mock · {m}</span>;
}

export function IBtn({ label, children, pressed, hint, onClick }: { label: string; children: ReactNode; pressed?: boolean; hint?: string; onClick?: () => void }) {
  return (
    <button type="button" className="ibtn" aria-label={label} title={label} aria-pressed={pressed} data-hint={hint ?? label} onClick={onClick}>
      {children}
    </button>
  );
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} className="switch" onClick={() => onChange(!on)} data-hint={label}>
      <span className="switch__thumb" />
    </button>
  );
}

export function Segmented({ options, value, onChange, label }: { options: string[]; value: string; onChange: (v: string) => void; label: string }) {
  const idx = options.indexOf(value);
  return (
    <div className="seg" role="radiogroup" aria-label={label} style={{ ["--seg-n" as string]: options.length, ["--seg-i" as string]: idx }}>
      <span className="seg__glide" aria-hidden />
      {options.map((o) => (
        <button key={o} type="button" role="radio" aria-checked={o === value} onClick={() => onChange(o)}>
          {o}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Panel frame
// ---------------------------------------------------------------------------

export function Panel({
  title, icon, meta, tools, children, region, className, led, index,
}: {
  title: string; icon?: ReactNode; meta?: ReactNode; tools?: ReactNode; children: ReactNode;
  region: string; className?: string; led?: "on" | "run" | "warn" | "err" | "off"; index?: string;
}) {
  return (
    <section className={`panel ${className ?? ""}`} data-region={region} aria-label={title}>
      <header className="panel__head">
        {index && <span className="panel__index">{index}</span>}
        {led && <Led state={led} />}
        {icon && <span className="panel__icon">{icon}</span>}
        <h2 className="panel__title">{title}</h2>
        {meta && <span className="panel__meta">{meta}</span>}
        <span className="panel__tools">{tools}</span>
      </header>
      <div className="panel__body">{children}</div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Explorer tree
// ---------------------------------------------------------------------------

export function Tree({ active, onOpen, tags = false }: { active: string; onOpen: (p: string) => void; tags?: boolean }) {
  const [open, setOpen] = useState<Set<string>>(() => new Set(TREE.filter((n) => n.open).map((n) => n.path)));
  const [focus, setFocus] = useState(active);
  const rows = useMemo(() => {
    const out: Node[] = [];
    for (const n of TREE) {
      const parent = n.path.split("/").slice(0, -1);
      let visible = true;
      for (let i = 1; i <= parent.length; i++) if (!open.has(parent.slice(0, i).join("/"))) visible = false;
      if (visible) out.push(n);
    }
    return out;
  }, [open]);
  const toggle = (p: string) => setOpen((s) => { const n = new Set(s); if (n.has(p)) n.delete(p); else n.add(p); return n; });
  return (
    <div
      className="tree"
      role="tree"
      aria-label="Files"
      tabIndex={0}
      aria-activedescendant={`row-${focus}`}
      onKeyDown={(e) => {
        const i = rows.findIndex((r) => r.path === focus);
        if (e.key === "ArrowDown") setFocus(rows[Math.min(rows.length - 1, i + 1)]!.path);
        else if (e.key === "ArrowUp") setFocus(rows[Math.max(0, i - 1)]!.path);
        else if (e.key === "Enter" || e.key === " ") { const r = rows[i]!; if (r.kind === "dir") toggle(r.path); else onOpen(r.path); }
        else return;
        e.preventDefault();
      }}
    >
      {rows.map((n) => {
        const isOpen = open.has(n.path);
        return (
          <div
            key={n.path}
            id={`row-${n.path}`}
            className="row"
            role="treeitem"
            aria-level={n.depth + 1}
            aria-expanded={n.kind === "dir" ? isOpen : undefined}
            aria-selected={n.path === active}
            data-focused={n.path === focus}
            data-ignored={n.ignored || undefined}
            data-kind={n.kind}
            style={{ ["--depth" as string]: n.depth }}
            onClick={() => { setFocus(n.path); if (n.kind === "dir") toggle(n.path); else onOpen(n.path); }}
            title={n.path}
          >
            {Array.from({ length: n.depth }, (_, i) => <span key={i} className="row__guide" style={{ ["--g" as string]: i }} aria-hidden />)}
            <span className="row__chev" data-open={isOpen} aria-hidden>{n.kind === "dir" && <I.Chevron />}</span>
            <span className="row__icon">{n.kind === "dir" ? (isOpen ? <I.FolderOpen /> : <I.Folder />) : <I.FileGlyph name={n.name} />}</span>
            <span className="row__name">{n.name}</span>
            {tags && n.tag && <span className="row__tag" data-tag={n.tag} aria-hidden />}
            {n.path === "crates/exec/src/tree.rs" && <span className="row__status" data-status="modified" title="Unsaved changes">●</span>}
            {n.path === "crates/exec/src/executor.rs" && <span className="row__status" data-status="warn" title="1 warning">1</span>}
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Editor tabs with a gliding active indicator (one element, transform only)
// ---------------------------------------------------------------------------

export function Tabs({ active, onActivate, numbered = false }: { active: string; onActivate: (p: string) => void; numbered?: boolean }) {
  const strip = useRef<HTMLDivElement>(null);
  const [glide, setGlide] = useState<{ x: number; w: number } | null>(null);
  useLayoutEffect(() => {
    const el = strip.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(active)}"]`);
    if (el) setGlide({ x: el.offsetLeft, w: el.offsetWidth });
  }, [active]);
  return (
    <div className="tabs" role="tablist" aria-label="Open documents" ref={strip}>
      {DOCS.map((d, i) => {
        const on = d.path === active;
        return (
          <div
            key={d.path}
            role="tab"
            aria-selected={on}
            tabIndex={on ? 0 : -1}
            className="tab"
            data-path={d.path}
            data-dirty={d.dirty || undefined}
            onClick={() => onActivate(d.path)}
            title={d.path}
            data-hint={`${d.path} — Ctrl+Tab cycles, middle-click closes`}
          >
            {numbered && <span className="tab__num">{String(i + 1).padStart(2, "0")}</span>}
            <I.FileGlyph name={d.title} />
            <span className="tab__title">{d.title}</span>
            <span className="tab__close" role="button" aria-label={`Close ${d.title}${d.dirty ? " (unsaved)" : ""}`}>
              {d.dirty ? <span className="tab__dirty" /> : <I.Close width={12} height={12} />}
            </span>
          </div>
        );
      })}
      {glide && <span className="tabs__glide" aria-hidden style={{ transform: `translateX(${glide.x}px) scaleX(${glide.w / 100})` }} />}
      <span className="tabs__rest" />
    </div>
  );
}

export function Crumbs() {
  return (
    <nav className="crumbs" aria-label="Location">
      {CRUMBS.map((c, i) => (
        <span key={i} className="crumb" data-symbol={i >= 4 || undefined}>
          {i > 0 && <I.Chevron className="crumb__sep" width={12} height={12} />}
          {c}
        </span>
      ))}
    </nav>
  );
}

// ---------------------------------------------------------------------------
// Monaco host: one editor, models per document, theme from the concept's tokens
// ---------------------------------------------------------------------------

export function Code({ active, concept, theme }: { active: string; concept: string; theme: string }) {
  const host = useRef<HTMLDivElement>(null);
  const ed = useRef<import("monaco-editor").editor.IStandaloneCodeEditor | null>(null);
  const models = useRef(new Map<string, import("monaco-editor").editor.ITextModel>());
  const fontRef = useRef<string>("");
  useEffect(() => {
    const monaco = setupMonaco();
    const el = host.current!;
    const css = getComputedStyle(el);
    fontRef.current = css.getPropertyValue("--font-code").trim();
    const name = `${concept}-${theme}`;
    themeFrom(el, name, theme === "light" ? "vs" : "vs-dark");
    const editor = monaco.editor.create(el, {
      model: null,
      theme: name,
      automaticLayout: true,
      fontFamily: fontRef.current,
      fontSize: parseFloat(css.getPropertyValue("--code-size")) || 13,
      lineHeight: parseFloat(css.getPropertyValue("--code-leading")) || 20,
      fontLigatures: true,
      minimap: { enabled: true, renderCharacters: false, maxColumn: 80, scale: 1 },
      scrollBeyondLastLine: false,
      renderLineHighlight: "all",
      padding: { top: 8 },
      smoothScrolling: true,
      cursorBlinking: "phase",
      cursorSmoothCaretAnimation: "off",
      guides: { indentation: true, bracketPairs: false },
      scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10, useShadows: false },
      overviewRulerBorder: false,
      stickyScroll: { enabled: false },
    });
    ed.current = editor;
    for (const d of DOCS) models.current.set(d.path, monaco.editor.createModel(d.text, d.language));
    // Fonts load asynchronously: remeasure when they are ready.
    void document.fonts.ready.then(() => monaco.editor.remeasureFonts());
    return () => {
      editor.dispose();
      for (const m of models.current.values()) m.dispose();
      models.current.clear();
    };
  }, [concept, theme]);
  useEffect(() => {
    const e = ed.current;
    const m = models.current.get(active);
    if (!e || !m) return;
    e.setModel(m);
    if (active === ACTIVE_FILE) {
      e.setPosition({ lineNumber: 84, column: 17 });
      e.revealLineNearTop(70);
    }
  }, [active, concept, theme]);
  return <div className="editor" ref={host} aria-label="Editor" />;
}

// ---------------------------------------------------------------------------
// Console: output stream + command prompt
// ---------------------------------------------------------------------------

export function Output({ live = true }: { live?: boolean }) {
  return (
    <div className="out" role="log" aria-live="polite" aria-label="Output">
      {OUTPUT.map((l, i) => (
        <div key={i} className="out__row" data-stream={l.stream} data-kind={l.kind}>
          <span className="out__gutter" aria-hidden>{l.stream === "stderr" ? "▏" : ""}</span>
          <span className="visually-hidden">{l.stream === "stderr" ? "stderr: " : ""}</span>
          <span className="out__text">
            {l.kind === "ok" ? (
              <>{l.text.replace(/ ok$/, " ")}<span className="out__ok">ok</span></>
            ) : l.text}
          </span>
        </div>
      ))}
      {live && (
        <div className="out__row out__row--live" data-kind="running">
          <span className="out__gutter" aria-hidden />
          <span className="out__text"><span className="out__caret" aria-hidden /></span>
        </div>
      )}
    </div>
  );
}

export function Prompt() {
  return (
    <div className="prompt">
      <span className="prompt__sigil" aria-hidden>$</span>
      <input className="prompt__input" aria-label="Command" defaultValue={TASK.label} spellCheck={false} data-hint="Command to run in the workspace — Enter runs, ↑ history" />
      <button type="button" className="btn btn--sm btn--danger-quiet" data-hint="Stop the running task — Shift+F5"><I.Stop width={12} height={12} /> Stop</button>
    </div>
  );
}

export function ConsoleTabs({ value }: { value: string }) {
  const tabs: [string, ReactNode, string?][] = [
    ["Output", <I.Terminal key="o" />],
    ["Problems", <I.Problems key="p" />, "M2"],
    ["Terminal", <I.Terminal key="t" />, "M4"],
  ];
  return (
    <div className="ctabs" role="tablist" aria-label="Console views">
      {tabs.map(([t, icon, m]) => (
        <button key={t} type="button" role="tab" aria-selected={t === value} className="ctab" data-future={m || undefined} title={m ? `${t} — arrives in ${m}` : t}>
          {icon}
          {t}
          {t === "Problems" && <span className="count">2</span>}
        </button>
      ))}
    </div>
  );
}

/** Indeterminate activity bar: a transform-only sweep, only while running. */
export function Activity({ running }: { running: boolean }) {
  return <div className="activity" data-running={running} aria-hidden><span /></div>;
}

// ---------------------------------------------------------------------------
// AI workspace (M3 mock)
// ---------------------------------------------------------------------------

export function AgentView({ compact = false }: { compact?: boolean }) {
  const [streamed, setStreamed] = useState(0);
  const tail = "Running the crate's tests with the patch applied; the hanging case should now finish within the grace period.";
  useEffect(() => {
    if (new URLSearchParams(location.search).has("still")) { setStreamed(tail.length); return; }
    let i = 0;
    const id = setInterval(() => {
      i = Math.min(tail.length, i + 3);
      setStreamed(i);
      if (i >= tail.length) clearInterval(id);
    }, 40);
    return () => clearInterval(id);
  }, []);
  return (
    <div className="agent" data-compact={compact || undefined}>
      <div className="agent__sessions" role="tablist" aria-label="Agent sessions">
        {AGENT_SESSIONS.map((s, i) => (
          <button key={s.id} type="button" role="tab" aria-selected={i === 0} className="sess" data-tag={s.tag} data-hint={`${s.title} — ${s.model}`}>
            <Led state={s.state === "waiting" ? "warn" : "off"} />
            <span className="sess__title">{s.title}</span>
            {s.state === "waiting" && <span className="sess__badge">1</span>}
          </button>
        ))}
        <IBtn label="New session"><I.Plus /></IBtn>
      </div>
      <div className="agent__log" role="log" aria-label="Conversation">
        {AGENT_STEPS.map((st, i) => <Step key={i} st={st} />)}
        <div className="step" data-kind="assistant">
          <p className="step__text">{tail.slice(0, streamed)}{streamed < tail.length && <span className="out__caret" />}</p>
        </div>
      </div>
      <div className="composer">
        <div className="composer__field" role="textbox" aria-label="Message the agent" tabIndex={0} data-hint="Message — Enter sends, Shift+Enter new line">
          <span className="composer__ph">Ask about this workspace, or describe a change…</span>
        </div>
        <div className="composer__bar">
          <span className="chip" data-hint="Context attached to the next message">executor.rs · L180–260</span>
          <span className="chip">claude-sonnet</span>
          <span className="composer__spacer" />
          <button type="button" className="btn btn--sm btn--primary" aria-label="Send" data-hint="Send — Enter"><I.Send width={13} height={13} /></button>
        </div>
      </div>
    </div>
  );
}

function Step({ st }: { st: (typeof AGENT_STEPS)[number] }) {
  switch (st.kind) {
    case "user":
      return <div className="step" data-kind="user"><p className="step__text">{st.text}</p></div>;
    case "assistant":
      return <div className="step" data-kind="assistant"><p className="step__text">{st.text}</p></div>;
    case "tool":
      return (
        <div className="step step--tool" data-kind="tool" data-state={st.state}>
          <span className="tool__state" aria-label={st.state}>{st.state === "done" ? <I.Check width={13} height={13} /> : <span className="spin" />}</span>
          <code className="tool__name">{st.tool}</code>
          <span className="tool__target">{st.target}</span>
          <span className="tool__meta">{st.meta}</span>
        </div>
      );
    case "checkpoint":
      return (
        <div className="step" data-kind="checkpoint">
          <span className="cp__line" aria-hidden />
          <I.Flag width={12} height={12} />
          <span>{st.text}</span>
          <span className="cp__meta">{st.meta}</span>
          <button type="button" className="link" data-hint="Restore the workspace to this checkpoint">Restore</button>
          <span className="cp__line" aria-hidden />
        </div>
      );
    case "diff":
      return (
        <div className="step diff" data-kind="diff">
          <div className="diff__head"><I.Diff width={13} height={13} /><span className="diff__file">{st.target}</span><span className="diff__stat"><span className="add">+6</span> <span className="del">−2</span></span></div>
          <div className="diff__body">
            {DIFF.map((r, i) => (
              <div key={i} className="diff__row" data-t={r.t}>
                <span className="diff__n">{r.n}</span>
                <span className="diff__sign" aria-hidden>{r.t === "add" ? "+" : r.t === "del" ? "−" : " "}</span>
                <span className="diff__code">{r.s}</span>
              </div>
            ))}
          </div>
        </div>
      );
    case "approval":
      return (
        <div className="step approval" data-kind="approval" role="group" aria-label="Permission request">
          <div className="approval__head"><I.Shield width={14} height={14} /><span>Allow <code>{st.tool}</code> on <code>{st.target!.split("/").pop()}</code>?</span></div>
          <div className="approval__meta">{st.meta} · a checkpoint exists for this turn</div>
          <div className="approval__actions">
            <button type="button" className="btn btn--sm btn--primary" data-hint="Apply this patch once — Enter">Allow once</button>
            <button type="button" className="btn btn--sm" data-hint="Allow fs.applyPatch for the rest of this session">Allow for session</button>
            <button type="button" className="btn btn--sm btn--quiet" data-hint="Reject; the agent is told why — Esc">Deny</button>
          </div>
        </div>
      );
  }
}

// ---------------------------------------------------------------------------
// Environment Manager (M2 mock)
// ---------------------------------------------------------------------------

export function EnvView() {
  return (
    <div className="envs">
      <div className="envs__toolbar">
        <h2 className="envs__title">Environments</h2>
        <MockBadge m="M2" />
        <span className="envs__spacer" />
        <button type="button" className="btn btn--sm"><I.Rerun width={13} height={13} /> Detect again</button>
      </div>
      <div className="envs__grid" role="table" aria-label="Toolchains for this workspace">
        <div className="envs__row envs__row--head" role="row">
          <span role="columnheader">Toolchain</span><span role="columnheader">Required by project</span><span role="columnheader">Detected</span><span role="columnheader">Source</span><span role="columnheader" />
        </div>
        {ENVIRONMENTS.map((e) => (
          <div key={e.id} className="envs__row" role="row" data-state={e.state}>
            <span role="cell" className="envs__name"><Led state={e.state === "ok" ? "on" : e.state === "installing" ? "run" : "err"} /><b>{e.name}</b><small>{e.tools}</small></span>
            <span role="cell" className="mono">{e.required}</span>
            <span role="cell" className="mono">{e.detected ?? "not found"}</span>
            <span role="cell" className="mono faint">{e.source}</span>
            <span role="cell" className="envs__act">
              {e.state === "ok" && <span className="ok-text"><I.Check width={13} height={13} /> Ready</span>}
              {e.state === "missing" && <button type="button" className="btn btn--sm btn--primary"><I.Download width={13} height={13} /> Plan install…</button>}
              {e.state === "installing" && (
                <span className="envs__progress">
                  <span className="meter" role="progressbar" aria-valuenow={62} aria-valuemin={0} aria-valuemax={100} aria-label="Installing CMake"><span style={{ transform: `scaleX(${e.progress})` }} /></span>
                  <small>{e.step}</small>
                </span>
              )}
            </span>
          </div>
        ))}
      </div>
      <div className="envs__diag">
        <h3 className="envs__sub">Build diagnostics <span className="count">2</span></h3>
        {DIAGNOSTICS.map((d, i) => (
          <div key={i} className="diag" data-sev={d.sev}>
            {d.sev === "error" ? <I.Err width={14} height={14} /> : <I.Warn width={14} height={14} />}
            <span className="diag__msg">{d.msg}</span>
            <span className="diag__code mono">{d.code}</span>
            <span className="diag__loc mono">{d.file}:{d.line}:{d.col}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

export function SettingsView() {
  const [theme, setTheme] = useState("System");
  const [wrap, setWrap] = useState(false);
  const [reopen, setReopen] = useState(true);
  const [motion, setMotion] = useState("Standard");
  return (
    <div className="settings">
      <nav className="settings__nav" aria-label="Settings sections">
        {["Appearance", "Editor", "Workspace", "Keyboard", "Agents · M3", "Environments · M2", "About"].map((s, i) => (
          <button key={s} type="button" className="settings__navitem" aria-current={i === 0 ? "page" : undefined} data-future={s.includes("·") || undefined}>{s}</button>
        ))}
      </nav>
      <div className="settings__body">
        <h2 className="settings__h">Appearance</h2>
        <div className="setting">
          <div className="setting__label"><b>Theme</b><small>Follow the operating system, or force light or dark.</small></div>
          <Segmented label="Theme" options={["System", "Light", "Dark"]} value={theme} onChange={setTheme} />
        </div>
        <div className="setting">
          <div className="setting__label"><b>Motion</b><small>Reduced removes movement and keeps fades under 100 ms. The OS setting always wins.</small></div>
          <Segmented label="Motion" options={["Standard", "Reduced", "Off"]} value={motion} onChange={setMotion} />
        </div>
        <h2 className="settings__h">Editor</h2>
        <div className="setting">
          <div className="setting__label"><b>Font size</b><small>Applies to the editor and console.</small></div>
          <div className="stepper" role="group" aria-label="Font size">
            <button type="button" className="ibtn" aria-label="Smaller">−</button>
            <input className="field field--num" aria-label="Font size in pixels" defaultValue="13" />
            <button type="button" className="ibtn" aria-label="Larger">+</button>
          </div>
        </div>
        <div className="setting">
          <div className="setting__label"><b>Font family</b><small>Empty uses the theme's code face.</small></div>
          <input className="field" aria-label="Editor font family" placeholder="JetBrains Mono" />
        </div>
        <div className="setting">
          <div className="setting__label"><b>Word wrap</b><small>Wrap long lines at the viewport edge.</small></div>
          <Switch label="Word wrap" on={wrap} onChange={setWrap} />
        </div>
        <h2 className="settings__h">Workspace</h2>
        <div className="setting">
          <div className="setting__label"><b>Reopen last workspace</b><small>Restore the folder, tabs and expanded directories on launch.</small></div>
          <Switch label="Reopen last workspace" on={reopen} onChange={setReopen} />
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Command palette with gliding selection
// ---------------------------------------------------------------------------

export function Palette({ open, mode, onClose }: { open: boolean; mode: "commands" | "files"; onClose: () => void }) {
  const { mounted, phase } = usePresence(open, 120);
  const [q, setQ] = useState(mode === "files" ? "exec" : "");
  const [i, setI] = useState(0);
  const items = mode === "commands" ? COMMANDS.filter((c) => c.label.toLowerCase().includes(q.toLowerCase())) : FILE_RESULTS;
  useEffect(() => { if (open) { setQ(mode === "files" ? "exec" : ""); setI(0); } }, [open, mode]);
  if (!mounted) return null;
  return (
    <div className="scrim" data-phase={phase} onPointerDown={onClose}>
      <div className="palette" role="dialog" aria-label="Command palette" data-phase={phase} onPointerDown={(e) => e.stopPropagation()}>
        <div className="palette__bar">
          <I.Search className="palette__icon" />
          <input
            className="palette__input"
            autoFocus
            value={q}
            onChange={(e) => { setQ(e.target.value); setI(0); }}
            placeholder={mode === "commands" ? "Run a command…" : "Go to a file…"}
            role="combobox"
            aria-expanded
            aria-controls="pal-list"
            aria-activedescendant={`pal-${i}`}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") setI((x) => Math.min(items.length - 1, x + 1));
              else if (e.key === "ArrowUp") setI((x) => Math.max(0, x - 1));
              else if (e.key === "Escape") onClose();
              else return;
              e.preventDefault();
            }}
          />
          <span className="palette__mode">{mode === "commands" ? "Commands" : "Files"}</span>
        </div>
        <ul className="palette__list" role="listbox" id="pal-list" style={{ ["--sel" as string]: i }}>
          <span className="palette__glide" aria-hidden />
          {mode === "commands"
            ? (items as typeof COMMANDS).map((c, n) => (
                <li key={c.label} id={`pal-${n}`} role="option" aria-selected={n === i} className="palette__item" onPointerMove={() => setI(n)}>
                  <span className="palette__group">{c.group}</span>
                  <span className="palette__label">{c.label}</span>
                  {c.key && <Kbd k={c.key} />}
                </li>
              ))
            : (items as typeof FILE_RESULTS).map((r, n) => {
                const name = r.path.split("/").pop()!;
                const dir = r.path.slice(0, -name.length - 1);
                return (
                  <li key={r.path} id={`pal-${n}`} role="option" aria-selected={n === i} className="palette__item" onPointerMove={() => setI(n)}>
                    <I.FileGlyph name={name} />
                    <span className="palette__label">{name}</span>
                    <span className="palette__dir">{dir.slice(0, r.match[0])}<mark>{dir.slice(r.match[0], r.match[1] + 1)}</mark>{dir.slice(r.match[1] + 1)}</span>
                  </li>
                );
              })}
        </ul>
        <div className="palette__foot">
          <span><Kbd k="↑ ↓" /> move</span><span><Kbd k="↵" /> run</span><span><Kbd k="Esc" /> close</span>
          <span className="palette__foot-spacer" />
          <span>{mode === "commands" ? "Ctrl P for files" : "Ctrl K for commands"}</span>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

export function Toast({ open, children }: { open: boolean; children: ReactNode }) {
  const { mounted, phase } = usePresence(open, 160);
  if (!mounted) return null;
  return <div className="toast" role="status" data-phase={phase}>{children}</div>;
}

// ---------------------------------------------------------------------------
// Component sheet: every control in its states
// ---------------------------------------------------------------------------

export function ComponentSheet() {
  const [sw, setSw] = useState(true);
  const [seg, setSeg] = useState("Split");
  return (
    <div className="sheet">
      <section className="sheet__group">
        <h3>Buttons</h3>
        <div className="sheet__row">
          <button type="button" className="btn btn--primary">Run task</button>
          <button type="button" className="btn">Save all</button>
          <button type="button" className="btn btn--quiet">Cancel</button>
          <button type="button" className="btn btn--danger">Discard changes</button>
          <button type="button" className="btn" disabled>Disabled</button>
          <button type="button" className="btn is-hover">Hover</button>
          <button type="button" className="btn is-active">Pressed</button>
          <button type="button" className="btn is-focus">Focused</button>
          <button type="button" className="btn btn--primary is-loading" aria-busy="true"><span className="spin" /> Installing</button>
        </div>
        <div className="sheet__row">
          <IBtn label="Toggle side bar" pressed><I.Sidebar /></IBtn>
          <IBtn label="Toggle panel"><I.PanelBottom /></IBtn>
          <IBtn label="Agent dock"><I.PanelRight /></IBtn>
          <IBtn label="Settings"><I.Gear /></IBtn>
          <span className="sheet__sep" />
          <Led state="on" label="connected" /><Led state="run" label="running" /><Led state="warn" label="waiting" /><Led state="err" label="failed" /><Led state="off" label="idle" />
          <span className="sheet__sep" />
          <span className="badge">Rust</span><span className="badge badge--accent">Default</span><MockBadge m="M3" /><span className="count">12</span>
          <Kbd k="Ctrl Shift R" />
        </div>
      </section>
      <section className="sheet__group">
        <h3>Inputs and selection</h3>
        <div className="sheet__row">
          <input className="field" placeholder="Search files" aria-label="Search files" />
          <input className="field is-focus" defaultValue="cargo test -p axiom-exec" aria-label="Focused field" />
          <input className="field" aria-invalid="true" defaultValue="cargo tset" aria-label="Invalid field" />
          <button type="button" className="select" aria-haspopup="listbox">Rust 1.99.1 <I.ChevronDown width={13} height={13} /></button>
        </div>
        <div className="sheet__row">
          <Switch label="Word wrap" on={sw} onChange={setSw} />
          <Switch label="Minimap" on={false} onChange={() => {}} />
          <Segmented label="Layout" options={["Single", "Split", "Grid"]} value={seg} onChange={setSeg} />
          <label className="check"><input type="checkbox" defaultChecked /> <span>Show ignored files</span></label>
        </div>
      </section>
      <section className="sheet__group">
        <h3>Progress and feedback</h3>
        <div className="sheet__row sheet__row--col">
          <div className="meter" role="progressbar" aria-label="Download" aria-valuenow={62}><span style={{ transform: "scaleX(.62)" }} /></div>
          <Activity running />
          <div className="toast toast--static" role="status"><I.Check width={14} height={14} /> Saved <b>executor.rs</b> <span className="faint">· 4.1 KB through axiomd</span><button type="button" className="link">Undo</button></div>
          <div className="toast toast--static" data-tone="err" role="alert"><I.Err width={14} height={14} /> <b>cargo test</b> failed with exit code 101 <button type="button" className="link">Show output</button></div>
        </div>
      </section>
      <section className="sheet__group">
        <h3>Menu</h3>
        <div className="menu menu--static" role="menu" aria-label="File">
          <div className="menu__item" role="menuitem"><span>Open Folder…</span><Kbd k="Ctrl O" /></div>
          <div className="menu__item" role="menuitem" data-highlighted><span>Save</span><Kbd k="Ctrl S" /></div>
          <div className="menu__item" role="menuitem" aria-disabled="true"><span>Save All</span></div>
          <div className="menu__sep" role="separator" />
          <div className="menu__label">Recent</div>
          <div className="menu__item" role="menuitem"><span>axiom</span><span className="menu__aside">~/src/axiom</span></div>
          <div className="menu__item" role="menuitem"><span>tauri-playground</span><span className="menu__aside">~/src/tp</span></div>
        </div>
      </section>
    </div>
  );
}
