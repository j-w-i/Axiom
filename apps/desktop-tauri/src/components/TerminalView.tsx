// The console's Terminal view (M4-TERM-002; ADR-0018 §4; workspace-spec §2.4): the workspace's
// terminals as tabs, each an xterm.js screen. The screens live outside React (a terminal keeps
// its screen while another view or tab shows) and are moved into the view when shown. Colours
// come from the tokens (`--ansi-*`), the font is the code font. Keys go to the shell except the
// app's global chords; copy and paste use the platform's terminal chords.

import { FitAddon } from "@xterm/addon-fit";
import { Terminal as Xterm, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useSyncExternalStore } from "react";
import type { TerminalsStore } from "../state/terminals";
import { substituteMod } from "../ui/hint";
import * as Icon from "../ui/icons";
import { IconButton } from "../ui/primitives";
import "../styles/components/terminal.css";

const IS_MAC = typeof navigator !== "undefined" && /mac/i.test(navigator.platform);

interface Screen {
  term: Xterm;
  fit: FitAddon;
  host: HTMLDivElement;
  dispose: () => void;
}
const screens = new Map<string, Screen>();

function css(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** The xterm theme of the current appearance, from the tokens. */
export function terminalTheme(): ITheme {
  const a = (n: string) => css(`--ansi-${n}`);
  return {
    background: css("--surface-panel"),
    foreground: css("--text"),
    cursor: css("--editor-cursor") || css("--text"),
    cursorAccent: css("--surface-panel"),
    selectionBackground: css("--editor-selection"),
    black: a("black"), red: a("red"), green: a("green"), yellow: a("yellow"), blue: a("blue"), magenta: a("magenta"), cyan: a("cyan"), white: a("white"),
    brightBlack: a("bright-black"), brightRed: a("bright-red"), brightGreen: a("bright-green"), brightYellow: a("bright-yellow"),
    brightBlue: a("bright-blue"), brightMagenta: a("bright-magenta"), brightCyan: a("bright-cyan"), brightWhite: a("bright-white"),
  };
}

/** Whether a key event is one of the app's global chords, which the terminal must not take. */
export function isAppChord(e: { key: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }, mac = IS_MAC): boolean {
  const mod = mac ? e.metaKey : e.ctrlKey;
  if (!mod || e.altKey) return false;
  const k = e.key.toLowerCase();
  // Mod+K palette, Mod+P files, Mod+J console, Mod+B side bar, Mod+` terminal, Mod+, settings,
  // Mod+L AI dock. On macOS every Cmd chord is the app's (Ctrl chords go to the shell).
  if (mac) return true;
  return !e.shiftKey && ["k", "p", "j", "b", "`", ",", "l"].includes(k);
}

/** Copy (selection) and paste chords of a terminal: Cmd+C/V on macOS, Ctrl+Shift+C/V elsewhere
 * (Ctrl+C stays the interrupt). */
export function clipboardChord(e: { key: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }, mac = IS_MAC): "copy" | "paste" | null {
  const k = e.key.toLowerCase();
  const on = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && e.shiftKey && !e.metaKey;
  if (!on) return null;
  return k === "c" ? "copy" : k === "v" ? "paste" : null;
}

function screenFor(store: TerminalsStore, id: string, label: string): Screen {
  const existing = screens.get(id);
  if (existing) return existing;
  const host = document.createElement("div");
  host.className = "terminal__screen";
  const fontSize = parseFloat(css("--code-size")) || 13;
  const term = new Xterm({
    fontFamily: css("--font-code") || "monospace",
    fontSize,
    lineHeight: 1.2,
    cursorBlink: false,
    scrollback: 5000,
    screenReaderMode: true,
    allowProposedApi: false,
    theme: terminalTheme(),
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== "keydown") return true;
    if (isAppChord(e)) return false;
    const clip = clipboardChord(e);
    if (clip === "copy") {
      if (term.hasSelection()) void navigator.clipboard.writeText(term.getSelection());
      return false;
    }
    if (clip === "paste") {
      void navigator.clipboard.readText().then((t) => t && term.paste(t)).catch(() => {});
      return false;
    }
    return true;
  });
  term.open(host);
  term.textarea?.setAttribute("aria-label", `Terminal: ${label}`);
  const subs = [
    term.onData((d) => store.write(id, d)),
    term.onResize(({ cols, rows }) => store.resize(id, cols, rows)),
  ];
  const off = store.onTerminalOutput(id, (data) => term.write(data));
  const retheme = new MutationObserver(() => (term.options.theme = terminalTheme()));
  retheme.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  const s: Screen = {
    term,
    fit,
    host,
    dispose: () => {
      off();
      for (const d of subs) d.dispose();
      retheme.disconnect();
      term.dispose();
      screens.delete(id);
    },
  };
  screens.set(id, s);
  return s;
}

/** Focuses the shown terminal (Mod+`). */
export function focusTerminal(id: string | null): void {
  if (id) screens.get(id)?.term.focus();
}

export function TerminalView({ store, id, visible }: { store: TerminalsStore; id: string; visible: boolean }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const body = useRef<HTMLDivElement>(null);
  const creating = useRef(false);

  // Shown with no terminal: start one.
  useEffect(() => {
    if (!visible || s.terminals.length > 0 || creating.current) return;
    creating.current = true;
    const el = body.current;
    void store.create(el ? Math.floor(el.clientWidth / 8) : 80, el ? Math.floor(el.clientHeight / 17) : 24).finally(() => (creating.current = false));
  }, [visible, s.terminals.length, store]);

  // The active terminal's screen in the body, fitted to it (and refitted on resize).
  const active = s.terminals.find((t) => t.terminalId === s.active) ?? null;
  const label = active ? store.label(active) : "";
  useEffect(() => {
    const el = body.current;
    if (!el || !active || !visible) return;
    const sc = screenFor(store, active.terminalId, label);
    if (sc.host.parentElement !== el) el.replaceChildren(sc.host);
    const refit = () => {
      if (el.clientWidth === 0 || el.clientHeight === 0) return;
      try {
        sc.fit.fit();
      } catch {
        /* not laid out yet */
      }
    };
    refit();
    const ro = new ResizeObserver(refit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [active, visible, store, label]);

  // Screens of closed terminals go.
  useEffect(() => {
    for (const [tid, sc] of [...screens]) if (!s.terminals.some((t) => t.terminalId === tid)) sc.dispose();
  }, [s.terminals]);

  return (
    <div className="terminal" id={id} role="tabpanel" aria-label="Terminal" data-testid="terminal" hidden={!visible}>
      <div className="terminal__bar">
        <div className="terminal__tabs" role="tablist" aria-label="Terminals">
          {s.terminals.map((t) => {
            const name = store.label(t);
            return (
              <span key={t.terminalId} className="terminal__tab" data-active={t.terminalId === s.active || undefined} data-ended={!t.running || undefined}>
                <button
                  type="button"
                  role="tab"
                  className="terminal__tab-name"
                  aria-selected={t.terminalId === s.active}
                  data-terminal={t.terminalId}
                  data-hint={`Show ${name}`}
                  onClick={() => {
                    store.activate(t.terminalId);
                    requestAnimationFrame(() => focusTerminal(t.terminalId));
                  }}
                >
                  {name}
                </button>
                <IconButton label={`Close ${name}`} hint={`Close ${name}`} onClick={() => void store.close(t.terminalId)}>
                  <Icon.Close />
                </IconButton>
              </span>
            );
          })}
        </div>
        <IconButton
          label="New terminal"
          hint={`New terminal — ${substituteMod("Mod")}+Shift+\``}
          onClick={() => {
            const el = body.current;
            void store.create(el ? Math.floor(el.clientWidth / 8) : 80, el ? Math.floor(el.clientHeight / 17) : 24).then((tid) => requestAnimationFrame(() => focusTerminal(tid)));
          }}
          data-testid="terminal-new"
        >
          <Icon.Plus />
        </IconButton>
      </div>
      {s.error && (
        <p className="terminal__error" role="alert">
          {s.error}
        </p>
      )}
      <div ref={body} className="terminal__body" />
    </div>
  );
}
