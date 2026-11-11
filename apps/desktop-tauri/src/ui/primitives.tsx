// Design-system components (docs/migration/qt-to-tauri/design-system.md).
// Accessible behaviour comes from Radix primitives where it is non-trivial
// (dialogs, menus: focus trapping, Escape, typeahead, ARIA); the rest are thin,
// token-styled elements.

import * as RDialog from "@radix-ui/react-dialog";
import * as RMenu from "@radix-ui/react-dropdown-menu";
import { useCallback, useEffect, useRef, type ButtonHTMLAttributes, type ReactNode } from "react";

type ButtonVariant = "default" | "primary" | "quiet" | "danger";

export function Button({
  variant = "default",
  size,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm" }) {
  const cls = ["btn", variant !== "default" && `btn--${variant}`, size && `btn--${size}`, className]
    .filter(Boolean)
    .join(" ");
  return <button type="button" className={cls} {...rest} />;
}

/** Icon-only button: `label` is mandatory and becomes aria-label and tooltip. */
export function IconButton({
  label,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; children: ReactNode }) {
  return (
    <button type="button" className="icon-btn" aria-label={label} title={label} {...rest}>
      {children}
    </button>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}

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
    <div className={`banner banner--${variant}`} role={variant === "danger" ? "alert" : "status"}>
      <span className="banner__text">{children}</span>
      {action && (
        <Button size="sm" onClick={onAction}>
          {action}
        </Button>
      )}
    </div>
  );
}

export interface DialogButton {
  text: string;
  variant?: ButtonVariant;
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
}: {
  open: boolean;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  buttons: DialogButton[];
  dismissAction: string;
  onChoose: (action: string) => void;
  testId?: string;
}) {
  return (
    <RDialog.Root open={open} onOpenChange={(o) => !o && onChoose(dismissAction)}>
      <RDialog.Portal>
        <RDialog.Overlay className="overlay" />
        <RDialog.Content className="dialog" data-testid={testId} aria-describedby={undefined}>
          <RDialog.Title className="dialog__title">{title}</RDialog.Title>
          {description && <RDialog.Description className="dialog__body">{description}</RDialog.Description>}
          {children}
          <div className="dialog__actions">
            {buttons.map((b) => (
              <Button
                key={b.action}
                variant={b.variant}
                data-action={b.action}
                autoFocus={b.variant === "primary"}
                onClick={() => onChoose(b.action)}
              >
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
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  children: ReactNode;
}) {
  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="overlay" />
        <RDialog.Content className="dialog" aria-describedby={undefined}>
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

export interface MenuEntry {
  label?: string;
  shortcut?: string;
  disabled?: boolean;
  onSelect?: () => void;
  separator?: boolean;
  heading?: string;
}

export function Menu({ trigger, items, label }: { trigger: ReactNode; items: MenuEntry[]; label: string }) {
  return (
    <RMenu.Root>
      <RMenu.Trigger asChild>
        <button type="button" className="btn btn--quiet btn--sm" aria-label={label}>
          {trigger}
        </button>
      </RMenu.Trigger>
      <RMenu.Portal>
        <RMenu.Content className="menu" align="start" sideOffset={4}>
          {items.map((it, i) =>
            it.separator ? (
              <RMenu.Separator key={i} className="menu__sep" />
            ) : it.heading ? (
              <RMenu.Label key={i} className="menu__label">
                {it.heading}
              </RMenu.Label>
            ) : (
              <RMenu.Item key={i} className="menu__item" disabled={it.disabled} onSelect={it.onSelect}>
                <span>{it.label}</span>
                {it.shortcut && <span className="menu__shortcut">{it.shortcut}</span>}
              </RMenu.Item>
            ),
          )}
        </RMenu.Content>
      </RMenu.Portal>
    </RMenu.Root>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={o.value === value} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** A draggable divider; reports the new size of the pane it controls. */
export function Splitter({
  orientation,
  size,
  min,
  max,
  invert = false,
  onResize,
  label,
}: {
  orientation: "vertical" | "horizontal";
  size: number;
  min: number;
  max: number;
  invert?: boolean;
  onResize: (size: number) => void;
  label: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const start = useRef<{ pos: number; size: number } | null>(null);
  const clamp = useCallback((v: number) => Math.max(min, Math.min(max, v)), [min, max]);
  useEffect(() => {
    const move = (e: PointerEvent) => {
      if (!start.current) return;
      const pos = orientation === "vertical" ? e.clientX : e.clientY;
      const delta = (pos - start.current.pos) * (invert ? -1 : 1);
      onResize(clamp(start.current.size + delta));
    };
    const up = () => {
      start.current = null;
      if (ref.current) ref.current.dataset.dragging = "false";
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
  }, [orientation, invert, onResize, clamp]);
  return (
    <div
      ref={ref}
      className={`splitter splitter--${orientation === "vertical" ? "v" : "h"}`}
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      aria-valuenow={Math.round(size)}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={(e) => {
        start.current = { pos: orientation === "vertical" ? e.clientX : e.clientY, size };
        if (ref.current) ref.current.dataset.dragging = "true";
        e.preventDefault();
      }}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 48 : 16;
        const dec = orientation === "vertical" ? "ArrowLeft" : "ArrowUp";
        const inc = orientation === "vertical" ? "ArrowRight" : "ArrowDown";
        if (e.key === dec) onResize(clamp(size + (invert ? step : -step)));
        if (e.key === inc) onResize(clamp(size + (invert ? -step : step)));
      }}
    />
  );
}
