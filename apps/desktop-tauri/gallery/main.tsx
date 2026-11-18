// Component gallery (dev only: `pnpm run gallery`, then /gallery.html; never in dist).
// Every ui/ control in every state of design-system.md §8, in the current theme;
// the toggles switch theme and motion level. Hover, pressed and focus are forced
// with data-force so a screenshot shows them. Marker for the build test: axiom-component-gallery.

import { StrictMode, useState, type CSSProperties, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import "../src/styles/fonts.css";
import "../src/styles/tokens.css";
import * as Icon from "../src/ui/icons";
import {
  Badge,
  Button,
  Count,
  Field,
  IconButton,
  Kbd,
  Keys,
  Led,
  Meter,
  RateMeter,
  Segmented,
  Spinner,
  Stepper,
  Switch,
} from "../src/ui/primitives";
import { Banner, ContextMenu, Dialog, Menu, Tooltip, ToastView } from "../src/ui/overlays";
import "./gallery.css";

const STATES = ["rest", "hover", "pressed", "focus", "disabled", "loading"] as const;
type State = (typeof STATES)[number];

function force(state: State): Record<string, string | boolean | undefined> {
  return state === "hover" || state === "pressed" || state === "focus" ? { "data-force": state } : {};
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="g__section" aria-label={title} data-gallery-section={title}>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

function Grid({ cols, heads, children }: { cols: number; heads: string[]; children: ReactNode }) {
  return (
    <div className="g__grid" style={{ "--cols": cols } as CSSProperties}>
      <span />
      {heads.map((h) => (
        <span key={h} className="g__head">
          {h}
        </span>
      ))}
      {children}
    </div>
  );
}

const noop = () => {};

function Buttons() {
  const variants = ["default", "primary", "quiet", "danger", "danger-quiet"] as const;
  return (
    <Section title="Button">
      <Grid cols={STATES.length} heads={[...STATES]}>
        {variants.flatMap((v) => [
          <span key={`${v}-l`} className="g__row-label">{v}</span>,
          ...STATES.map((s) => (
            <span key={`${v}-${s}`}>
              <Button variant={v} disabled={s === "disabled"} loading={s === "loading"} icon={<Icon.Search />} onClick={noop} {...force(s)}>
                Run
              </Button>
            </span>
          )),
        ])}
        <span className="g__row-label">sm</span>
        {STATES.map((s) => (
          <span key={`sm-${s}`}>
            <Button size="sm" disabled={s === "disabled"} loading={s === "loading"} onClick={noop} {...force(s)}>
              Restart backend
            </Button>
          </span>
        ))}
      </Grid>
    </Section>
  );
}

function IconButtons() {
  const states = ["rest", "hover", "pressed", "focus", "engaged", "disabled"] as const;
  return (
    <Section title="IconButton">
      <Grid cols={states.length} heads={[...states]}>
        <span className="g__row-label">toggle</span>
        {states.map((s) => (
          <span key={s}>
            <IconButton
              label={`Toggle side bar (${s})`}
              pressed={s === "engaged"}
              disabled={s === "disabled"}
              {...(s === "hover" || s === "pressed" || s === "focus" ? { "data-force": s } : {})}
            >
              <Icon.Sidebar />
            </IconButton>
          </span>
        ))}
      </Grid>
    </Section>
  );
}

function Indicators() {
  return (
    <Section title="Kbd, Keys, Badge, Count, Led, Spinner, Meter">
      <Grid cols={6} heads={["", "", "", "", "", ""]}>
        <span className="g__row-label">keys</span>
        <Kbd>F5</Kbd>
        <Keys keys="Ctrl K" />
        <Keys keys="Ctrl Shift R" />
        <Keys keys="Alt F4" />
        <span />
        <span />
        <span className="g__row-label">badge / count</span>
        <Badge>changed on disk</Badge>
        <Badge variant="accent">running</Badge>
        <Count value={3} label="3 problems" />
        <Count value={128} label="128 lines" />
        <span />
        <span />
        <span className="g__row-label">led</span>
        {(["off", "on", "run", "warn", "err"] as const).map((s) => (
          <span key={s} style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
            <Led state={s} label={`Task ${s}`} /> {s}
          </span>
        ))}
        <span />
        <span className="g__row-label">spinner</span>
        <Spinner />
        <span />
        <span />
        <span />
        <span />
        <span />
        <span className="g__row-label">meter</span>
        {[0, 0.33, 0.8, 1].map((v) => (
          <span key={v} style={{ width: 120 }}>
            <Meter value={v} label={`Progress ${Math.round(v * 100)}%`} />
          </span>
        ))}
        <RateMeter levels={[0.1, 0.3, 0.6, 1, 0.8, 0.5, 0.4, 0.2, 0.1, 0.05, 0.3, 0.7, 0.9, 0.6, 0.3, 0.1]} label="Output rate" />
        <span />
      </Grid>
    </Section>
  );
}

function Inputs() {
  const [sw, setSw] = useState(true);
  const [seg, setSeg] = useState<"system" | "light" | "dark">("dark");
  const [size, setSize] = useState(13);
  const themes = [{ value: "system" as const, label: "System" }, { value: "light" as const, label: "Light" }, { value: "dark" as const, label: "Dark" }];
  return (
    <Section title="Field, Switch, Segmented, Stepper">
      <div className="g__stack">
        <Grid cols={5} heads={["rest", "focus", "invalid", "disabled", "mono"]}>
          <span className="g__row-label">field</span>
          <Field placeholder="Platform monospace" aria-label="Field rest" />
          <Field defaultValue="IBM Plex Mono" aria-label="Field focus" data-force="focus" />
          <span>
            <Field defaultValue="-3" invalid describedBy="g-err" aria-label="Field invalid" />
            <div id="g-err" style={{ color: "var(--danger)", fontSize: "var(--text-small)" }}>
              Must be between 8 and 40
            </div>
          </span>
          <Field defaultValue="13" disabled aria-label="Field disabled" />
          <Field mono defaultValue="cargo test -p axiom-exec" aria-label="Field mono" />
        </Grid>
        <Grid cols={5} heads={["off", "on", "interactive", "disabled off", "disabled on"]}>
          <span className="g__row-label">switch</span>
          <Switch checked={false} onChange={noop} label="Switch off" />
          <Switch checked onChange={noop} label="Switch on" />
          <Switch checked={sw} onChange={setSw} label="Switch interactive" />
          <Switch checked={false} onChange={noop} label="Switch disabled" disabled />
          <Switch checked disabled onChange={noop} label="Switch on disabled" />
        </Grid>
        <Grid cols={5} heads={["system", "light", "dark", "interactive", "disabled"]}>
          <span className="g__row-label">segmented</span>
          {themes.map((t) => (
            <Segmented key={t.value} label={`Theme ${t.value}`} value={t.value} onChange={noop} options={themes} />
          ))}
          <Segmented label="Theme interactive" value={seg} onChange={setSeg} options={themes} />
          <Segmented label="Theme disabled" value="dark" onChange={noop} disabled options={themes} />
        </Grid>
        <Grid cols={3} heads={["at minimum", "interactive", "at maximum"]}>
          <span className="g__row-label">stepper</span>
          <Stepper label="Editor font size (min)" value={8} min={8} max={40} onChange={noop} />
          <Stepper label="Editor font size" value={size} min={8} max={40} onChange={setSize} />
          <Stepper label="Editor font size (max)" value={40} min={8} max={40} onChange={noop} />
        </Grid>
      </div>
    </Section>
  );
}

function Host({ title, children }: { title: string; children: (container: HTMLElement | null) => ReactNode }) {
  const [el, setEl] = useState<HTMLElement | null>(null);
  return (
    <div className="g__host">
      <span className="g__head">{title}</span>
      <div ref={setEl}>{el && children(el)}</div>
    </div>
  );
}

function Overlays() {
  const menu = [
    { label: "Open Folder…", shortcut: "Ctrl+O" },
    { separator: true },
    { heading: "Recent" },
    { label: "axiom", shortcut: "…/src/axiom" },
    { separator: true },
    { label: "Save", shortcut: "Ctrl+S" },
    { label: "Save All", disabled: true },
    { label: "Rerun Last", shortcut: "F5" },
  ];
  return (
    <Section title="Overlays: Menu, ContextMenu, Tooltip, Dialog, Toast, Banner">
      <div className="g__hosts">
        <Host title="menu (open)">{(c) => <Menu label="File menu" trigger="File" items={menu} open container={c} />}</Host>
        <Host title="context menu">
          {() => (
            <ContextMenu
              at={{ x: 0, y: 0 }}
              label="Output menu"
              onClose={noop}
              items={[{ label: "Copy selection", onSelect: noop }, { label: "Copy all", onSelect: noop }, { label: "Clear", onSelect: noop, disabled: true }]}
            />
          )}
        </Host>
        <Host title="tooltip (open)">
          {(c) => (
            <Tooltip label="Toggle panel" shortcut="Ctrl+J" open container={c}>
              <button type="button" className="icon-btn" aria-label="Toggle panel">
                <Icon.Panel />
              </button>
            </Tooltip>
          )}
        </Host>
        <Host title="dialog (open)">
          {(c) => (
            <Dialog
              open
              modal={false}
              container={c}
              title="executor.rs changed on disk"
              description="Someone else changed this file after you opened it. Overwrite replaces their version with yours; Reload discards your changes and shows theirs."
              dismissAction="cancel"
              buttons={[{ text: "Cancel", variant: "quiet", action: "cancel" }, { text: "Reload", action: "reload" }, { text: "Overwrite", variant: "primary", action: "overwrite" }]}
              onChoose={noop}
            />
          )}
        </Host>
        <Host title="toasts">
          {() => (
            <div className="g__toasts">
              <ToastView item={{ id: 1, tone: "ok", text: "cargo test finished: 24 passed" }} />
              <ToastView item={{ id: 2, tone: "err", text: "Could not save executor.rs: permission denied", action: { label: "Retry", run: noop } }} />
            </div>
          )}
        </Host>
        <Host title="banners">
          {() => (
            <div>
              <Banner variant="danger" action="Restart backend" onAction={noop}>The backend exited unexpectedly (exit code 3).</Banner>
              <Banner variant="warning" action="Dismiss" onAction={noop}>Could not list target/: permission denied.</Banner>
              <Banner variant="info">Settings are stored by the backend.</Banner>
            </div>
          )}
        </Host>
      </div>
    </Section>
  );
}

function Type() {
  const rows: [string, CSSProperties][] = [
    ["--text-title / 600", { fontSize: "var(--text-title)", lineHeight: "var(--leading-title)", fontWeight: 600 }],
    ["--text-body / 400", { fontSize: "var(--text-body)", lineHeight: "var(--leading-body)" }],
    ["--text-body / 500", { fontSize: "var(--text-body)", lineHeight: "var(--leading-body)", fontWeight: 500 }],
    ["--text-small / 600", { fontSize: "var(--text-small)", lineHeight: "var(--leading-small)", fontWeight: 600 }],
    ["--text-small / 400", { fontSize: "var(--text-small)", lineHeight: "var(--leading-small)" }],
    ["--text-tiny / 500", { fontSize: "var(--text-tiny)", lineHeight: "var(--leading-tiny)", fontWeight: 500 }],
    ["--text-body italic", { fontSize: "var(--text-body)", fontStyle: "italic" }],
  ];
  const mono: [string, CSSProperties][] = [
    ["--code-size / --code-leading", { fontFamily: "var(--font-code)", fontSize: "var(--code-size)", lineHeight: "var(--code-leading)" }],
    ["--code 500", { fontFamily: "var(--font-code)", fontSize: "var(--code-size)", fontWeight: 500 }],
    ["--code italic (comments)", { fontFamily: "var(--font-code)", fontSize: "var(--code-size)", fontStyle: "italic", color: "var(--syn-comment)" }],
    ["--out-size / --out-leading", { fontFamily: "var(--font-code)", fontSize: "var(--out-size)", lineHeight: "var(--out-leading)" }],
  ];
  return (
    <Section title="Type">
      <div className="g__type">
        {rows.map(([name, style]) => (
          <span key={name} style={{ display: "contents" }}>
            <code>{name}</code>
            <span style={style}>Open a folder to browse, edit and run it — 0123456789 · 12:04.3</span>
          </span>
        ))}
        {mono.map(([name, style]) => (
          <span key={name} style={{ display: "contents" }}>
            <code>{name}</code>
            <span className="g__editor" style={style}>
              {"fn spawn(cmd: &Command) -> Result<TaskId, ExecError> { 0O1lI|{}[] }"}
            </span>
          </span>
        ))}
      </div>
    </Section>
  );
}

type Motion = "standard" | "reduced" | "off";

function Gallery({ initialTheme, initialMotion }: { initialTheme: "dark" | "light"; initialMotion: Motion }) {
  const [theme, setTheme] = useState(initialTheme);
  const [motion, setMotion] = useState<Motion>(initialMotion);
  document.documentElement.dataset.theme = theme;
  document.documentElement.dataset.motion = motion;
  return (
    <main className="g" data-gallery="axiom-component-gallery">
      <div className="g__bar">
        <h1>Component gallery</h1>
        <Segmented label="Gallery theme" value={theme} onChange={setTheme} options={[{ value: "dark", label: "Dark" }, { value: "light", label: "Light" }]} />
        <Segmented label="Gallery motion" value={motion} onChange={setMotion} options={[{ value: "standard", label: "Standard" }, { value: "reduced", label: "Reduced" }, { value: "off", label: "Off" }]} />
      </div>
      <Buttons />
      <IconButtons />
      <Indicators />
      <Inputs />
      <Overlays />
      <Type />
    </main>
  );
}

// Initial theme and motion can come from the URL (?theme=light&motion=off), for screenshots.
const q = new URLSearchParams(location.search);
const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <Gallery
        initialTheme={q.get("theme") === "light" ? "light" : "dark"}
        initialMotion={q.get("motion") === "reduced" || q.get("motion") === "off" ? (q.get("motion") as Motion) : "standard"}
      />
    </StrictMode>,
  );
}
