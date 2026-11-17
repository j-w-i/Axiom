// Overlay primitives (design-system.md §4 overlay material, §9; M1.6-SYSTEM-004):
// Menu, ContextMenu, Tooltip, Dialog, Sheet, Toast and Banner. One material: a flat
// --surface-overlay box with a 1 px --line border, radius --r-3 and the single
// --overlay-shadow; scrim --scrim; no blur. Open/close is static for now: every root
// carries data-phase (default "in") for INTERACTION-001's presence helper.
// `container` (portal target), `open` and `modal` exist so the gallery can show the
// overlays in place; the app does not pass them.

import * as RDialog from "@radix-ui/react-dialog";
import * as RMenu from "@radix-ui/react-dropdown-menu";
import * as RTooltip from "@radix-ui/react-tooltip";
import { createContext, useContext, useEffect, useRef, useSyncExternalStore, type ReactElement, type ReactNode } from "react";
import { Button, Keys, Led } from "./primitives";
import "../styles/components/dialog.css";
import "../styles/components/menu.css";
import "../styles/components/tooltip.css";
import "../styles/components/toast.css";
import "../styles/components/banner.css";

export type Phase = "in" | "out";

// ---- shortcuts in menus and tooltips ----------------------------------------------

const CHORD = /^(?:(?:Ctrl|Cmd|⌘|Shift|⇧|Alt|⌥)\+)*(?:[A-Z0-9,.;/\\[\]`=-]|F\d{1,2}|Esc|Tab|Enter|Space|PgUp|PgDn|PageUp|PageDown|Home|End|Del)$/;

/** "Ctrl+Shift+R" → "Ctrl Shift R" when it is a key chord; null for any other hint text. */
export function chordKeys(shortcut: string): string | null {
  return CHORD.test(shortcut) ? shortcut.split("+").join(" ") : null;
}

function Shortcut({ shortcut }: { shortcut: string }) {
  const keys = chordKeys(shortcut);
  return keys ? (
    <span className="menu__keys">
      <Keys keys={keys} />
    </span>
  ) : (
    <span className="menu__hint">{shortcut}</span>
  );
}

// ---- Banner --------------------------------------------------------------------------

export function Banner({
  variant,
  children,
  action,
  onAction,
}: {
  variant: "info" | "warning" | "danger";
  children: ReactNode;
  action?: string;
  onAction?: () => void;
}) {
  return (
    <div className="banner" data-variant={variant} role={variant === "danger" ? "alert" : "status"}>
      <span className="banner__text">{children}</span>
      {action && (
        <Button size="sm" onClick={onAction}>
          {action}
        </Button>
      )}
    </div>
  );
}

// ---- Dialog, Sheet -------------------------------------------------------------------

export interface DialogButton {
  text: string;
  variant?: "default" | "primary" | "quiet" | "danger" | "danger-quiet";
  action: string;
}

/** Modal dialog with explicit buttons; Escape / outside click = `dismissAction`. */
export function Dialog({
  open,
  title,
  description,
  children,
  buttons,
  dismissAction,
  onChoose,
  testId,
  phase = "in",
  container,
  modal = true,
}: {
  open: boolean;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  buttons: DialogButton[];
  dismissAction: string;
  onChoose: (action: string) => void;
  testId?: string;
  phase?: Phase;
  container?: HTMLElement | null;
  modal?: boolean;
}) {
  return (
    <RDialog.Root open={open} modal={modal} onOpenChange={(o) => !o && onChoose(dismissAction)}>
      <RDialog.Portal container={container}>
        {modal && <RDialog.Overlay className="overlay" data-phase={phase} />}
        <RDialog.Content className="dialog" data-testid={testId} data-phase={phase} aria-describedby={undefined}>
          <RDialog.Title className="dialog__title">{title}</RDialog.Title>
          {description && <RDialog.Description className="dialog__body">{description}</RDialog.Description>}
          {children}
          <div className="dialog__actions">
            {buttons.map((b) => (
              <Button key={b.action} variant={b.variant} data-action={b.action} autoFocus={b.variant === "primary"} onClick={() => onChoose(b.action)}>
                {b.text}
              </Button>
            ))}
          </div>
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}

/** Non-blocking sheet (settings, about) closed with Escape or its button. */
export function Sheet({
  open,
  onOpenChange,
  title,
  children,
  phase = "in",
  container,
  modal = true,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  children: ReactNode;
  phase?: Phase;
  container?: HTMLElement | null;
  modal?: boolean;
}) {
  return (
    <RDialog.Root open={open} modal={modal} onOpenChange={onOpenChange}>
      <RDialog.Portal container={container}>
        {modal && <RDialog.Overlay className="overlay" data-phase={phase} />}
        <RDialog.Content className="dialog" data-phase={phase} aria-describedby={undefined}>
          <RDialog.Title className="dialog__title">{title}</RDialog.Title>
          {children}
          <div className="dialog__actions">
            <RDialog.Close asChild>
              <Button variant="primary">Done</Button>
            </RDialog.Close>
          </div>
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}

// ---- Menu ----------------------------------------------------------------------------

export interface MenuEntry {
  label?: string;
  shortcut?: string;
  disabled?: boolean;
  onSelect?: () => void;
  separator?: boolean;
  heading?: string;
}

export function Menu({
  trigger,
  items,
  label,
  phase = "in",
  open,
  container,
}: {
  trigger: ReactNode;
  items: MenuEntry[];
  label: string;
  phase?: Phase;
  open?: boolean;
  container?: HTMLElement | null;
}) {
  return (
    <RMenu.Root open={open} modal={open === undefined}>
      <RMenu.Trigger asChild>
        <button type="button" className="btn btn--quiet btn--sm" aria-label={label}>
          {trigger}
        </button>
      </RMenu.Trigger>
      <RMenu.Portal container={container}>
        <RMenu.Content className="menu" data-phase={phase} align="start" sideOffset={4}>
          {items.map((it, i) =>
            it.separator ? (
              <RMenu.Separator key={i} className="menu__sep" />
            ) : it.heading ? (
              <RMenu.Label key={i} className="menu__label">
                {it.heading}
              </RMenu.Label>
            ) : (
              <RMenu.Item key={i} className="menu__item" disabled={it.disabled} onSelect={it.onSelect}>
                <span className="menu__text">{it.label}</span>
                {it.shortcut && <Shortcut shortcut={it.shortcut} />}
              </RMenu.Item>
            ),
          )}
        </RMenu.Content>
      </RMenu.Portal>
    </RMenu.Root>
  );
}

// ---- ContextMenu ---------------------------------------------------------------------

/** Keyboard model of a menu: the index to focus next, "close", or null (not handled). Disabled items are skipped. */
export function menuKey(key: string, current: number, disabled: boolean[]): number | "close" | null {
  if (key === "Escape") return "close";
  const enabled = disabled.map((d, i) => (d ? -1 : i)).filter((i) => i >= 0);
  if (enabled.length === 0) return null;
  const pos = enabled.indexOf(current);
  if (key === "ArrowDown") return enabled[(pos + 1) % enabled.length]!;
  if (key === "ArrowUp") return enabled[(pos - 1 + enabled.length) % enabled.length] ?? enabled[enabled.length - 1]!;
  if (key === "Home") return enabled[0]!;
  if (key === "End") return enabled[enabled.length - 1]!;
  return null;
}

/**
 * A menu at the pointer (right-click), styled exactly like Menu. Focus moves to the first
 * item; arrows move, Enter activates, Escape or a click outside closes and returns focus
 * to where it was.
 */
export function ContextMenu({
  at,
  items,
  onClose,
  label,
  testId,
  phase = "in",
}: {
  at: { x: number; y: number } | null;
  items: { label: string; onSelect: () => void; disabled?: boolean }[];
  onClose: () => void;
  label: string;
  testId?: string;
  phase?: Phase;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const restore = useRef<HTMLElement | null>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (!at) return;
    restore.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ref.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not([aria-disabled="true"])')?.focus();
    const done = (refocus: boolean) => {
      close.current();
      if (refocus) restore.current?.focus();
    };
    const onPointer = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) done(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") done(true);
    };
    window.addEventListener("pointerdown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [at]);
  if (!at) return null;
  return (
    <div
      ref={ref}
      className="menu"
      role="menu"
      aria-label={label}
      data-testid={testId}
      data-phase={phase}
      style={{ position: "fixed", left: at.x, top: at.y }}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        const buttons = [...(ref.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])];
        const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = menuKey(e.key, current, items.map((it) => !!it.disabled));
        if (next === null || next === "close") return; // Escape is handled by the window listener
        e.preventDefault();
        buttons[next]?.focus();
      }}
    >
      {items.map((it) => (
        <button
          key={it.label}
          type="button"
          role="menuitem"
          className="menu__item"
          aria-disabled={it.disabled || undefined}
          data-disabled={it.disabled ? "" : undefined}
          tabIndex={-1}
          onClick={() => {
            if (it.disabled) return;
            it.onSelect();
            onClose();
            restore.current?.focus();
          }}
        >
          <span className="menu__text">{it.label}</span>
        </button>
      ))}
    </div>
  );
}

// ---- Tooltip -------------------------------------------------------------------------

const HasProvider = createContext(false);

/** One per window: 500 ms delay, instant when moving between tooltips within 300 ms (D8). */
export function TooltipProvider({ children }: { children: ReactNode }) {
  return (
    <RTooltip.Provider delayDuration={500} skipDelayDuration={300}>
      <HasProvider.Provider value>{children}</HasProvider.Provider>
    </RTooltip.Provider>
  );
}

/** Names a control (and its shortcut) on hover and on keyboard focus. The control keeps
 * its own accessible name: the tooltip is never the only way to learn it. */
export function Tooltip({
  label,
  shortcut,
  children,
  open,
  container,
  phase = "in",
}: {
  label: string;
  shortcut?: string;
  children: ReactElement;
  open?: boolean;
  container?: HTMLElement | null;
  phase?: Phase;
}) {
  const provided = useContext(HasProvider);
  const keys = shortcut ? chordKeys(shortcut) : null;
  const tip = (
    <RTooltip.Root open={open}>
      <RTooltip.Trigger asChild>{children}</RTooltip.Trigger>
      <RTooltip.Portal container={container}>
        <RTooltip.Content className="tooltip" data-phase={phase} side="bottom" sideOffset={6} collisionPadding={8}>
          <span>{label}</span>
          {keys && <Keys keys={keys} />}
        </RTooltip.Content>
      </RTooltip.Portal>
    </RTooltip.Root>
  );
  return provided ? tip : <TooltipProvider>{tip}</TooltipProvider>;
}

// ---- Toast ---------------------------------------------------------------------------

export interface ToastInput {
  tone: "ok" | "err";
  text: string;
  action?: { label: string; run: () => void };
}
export interface ToastItem extends ToastInput {
  id: number;
}

export const TOAST_MS = 4000;
export const TOAST_MAX = 3;

/** The toast queue: at most 3 visible (the oldest goes first), each dismissed after 4 s,
 * with the clock paused while the pointer or focus is on it. No app-store imports. */
export class ToastQueue {
  private items: ToastItem[] = [];
  private listeners = new Set<() => void>();
  private timers = new Map<number, { handle: ReturnType<typeof setTimeout> | null; left: number; since: number }>();
  private next = 1;

  subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };
  snapshot = (): ToastItem[] => this.items;

  push(t: ToastInput): number {
    const id = this.next++;
    this.items = [...this.items, { ...t, id }];
    while (this.items.length > TOAST_MAX) this.dismiss(this.items[0]!.id, false);
    this.start(id, TOAST_MS);
    this.emit();
    return id;
  }

  dismiss(id: number, emit = true): void {
    const t = this.timers.get(id);
    if (t?.handle) clearTimeout(t.handle);
    this.timers.delete(id);
    this.items = this.items.filter((x) => x.id !== id);
    if (emit) this.emit();
  }

  pause(id: number): void {
    const t = this.timers.get(id);
    if (!t || !t.handle) return;
    clearTimeout(t.handle);
    t.left = Math.max(0, t.left - (Date.now() - t.since));
    t.handle = null;
  }

  resume(id: number): void {
    const t = this.timers.get(id);
    if (t && !t.handle) this.start(id, t.left);
  }

  private start(id: number, ms: number): void {
    this.timers.set(id, { handle: setTimeout(() => this.dismiss(id), ms), left: ms, since: Date.now() });
  }
  private emit(): void {
    for (const fn of this.listeners) fn();
  }
}

export const toasts = new ToastQueue();
export const toast = (t: ToastInput): number => toasts.push(t);
export const useToasts = (queue: ToastQueue = toasts): ToastItem[] => useSyncExternalStore(queue.subscribe, queue.snapshot);

/** One toast. Never takes focus; announces as status (ok) or alert (err). */
export function ToastView({ item, onPause, onResume, onDismiss, phase = "in" }: { item: ToastItem; onPause?: () => void; onResume?: () => void; onDismiss?: () => void; phase?: Phase }) {
  return (
    <div
      className="toast"
      role={item.tone === "err" ? "alert" : "status"}
      data-tone={item.tone}
      data-phase={phase}
      onMouseEnter={onPause}
      onMouseLeave={onResume}
      onFocus={onPause}
      onBlur={onResume}
    >
      <Led state={item.tone === "err" ? "err" : "on"} />
      <span className="toast__text">{item.text}</span>
      {item.action && (
        <Button
          size="sm"
          variant="quiet"
          onClick={() => {
            item.action?.run();
            onDismiss?.();
          }}
        >
          {item.action.label}
        </Button>
      )}
    </div>
  );
}

/** The bottom-right stack, above the status bar. Mounted once by the app. */
export function ToastStack({ queue = toasts }: { queue?: ToastQueue }) {
  const items = useToasts(queue);
  return (
    <div className="toast-stack" aria-label="Notifications">
      {items.map((t) => (
        <ToastView key={t.id} item={t} onPause={() => queue.pause(t.id)} onResume={() => queue.resume(t.id)} onDismiss={() => queue.dismiss(t.id)} />
      ))}
    </div>
  );
}
