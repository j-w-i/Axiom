// Design-system components (docs/design/design-system.md §8–§9, M1.6-SYSTEM-003).
// Controls follow the material grammar: raised = pressable, recessed = holds a value
// or is engaged, flat = structure. State is expressed through ARIA and data-*
// attributes; each control has one stylesheet in src/styles/components/.
// Overlays (Dialog, Sheet, Menu) and Splitter are restyled by SYSTEM-004 / SHELL-002.
// ui/ imports nothing from state/, shell/, components/, daemon/ or @tauri-apps
// (tests/ui-imports.test.ts).

import {
  type ButtonHTMLAttributes,
  type CSSProperties,
  type InputHTMLAttributes,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import "../styles/components/button.css";
import "../styles/components/icon-button.css";
import "../styles/components/kbd.css";
import "../styles/components/badge.css";
import "../styles/components/led.css";
import "../styles/components/meter.css";
import "../styles/components/spinner.css";
import "../styles/components/field.css";
import "../styles/components/switch.css";
import "../styles/components/segmented.css";
import "../styles/components/stepper.css";
import { Tooltip } from "./overlays";

const IS_MAC = typeof navigator !== "undefined" && navigator.platform.toLowerCase().includes("mac");

// ---- pure keyboard rules (unit-tested) ------------------------------------------

/** Segmented / radio group: the option an arrow (or Home/End) key moves to, or null. */
export function segmentedTarget(key: string, index: number, count: number): number | null {
  if (count <= 0) return null;
  if (key === "ArrowRight" || key === "ArrowDown") return (index + 1) % count;
  if (key === "ArrowLeft" || key === "ArrowUp") return (index - 1 + count) % count;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  return null;
}

/** Stepper: the value an Up/Down/PageUp/PageDown key produces (clamped), or null. */
export function stepperTarget(key: string, value: number, { min, max, step = 1 }: { min: number; max: number; step?: number }): number | null {
  const clamp = (v: number) => Math.max(min, Math.min(max, v));
  if (key === "ArrowUp") return clamp(value + step);
  if (key === "ArrowDown") return clamp(value - step);
  if (key === "PageUp") return clamp(value + step * 10);
  if (key === "PageDown") return clamp(value - step * 10);
  if (key === "Home") return min;
  if (key === "End") return max;
  return null;
}

/** Keys that toggle a switch. */
export const isToggleKey = (key: string): boolean => key === " " || key === "Enter";

/** Keycap labels for a chord like "Ctrl Shift R"; on macOS Ctrl→⌘ and Alt→⌥. */
export function keyLabels(keys: string, mac = IS_MAC): string[] {
  return keys
    .split(/\s+/)
    .filter(Boolean)
    .map((k) => (mac && k === "Ctrl" ? "⌘" : mac && k === "Alt" ? "⌥" : k));
}

// ---- Spinner, Button, IconButton ------------------------------------------------

/** Stepped spinner; decorative (the owner carries aria-busy). */
export function Spinner() {
  return <span className="spinner" aria-hidden="true" />;
}

type ButtonVariant = "default" | "primary" | "quiet" | "danger" | "danger-quiet";

export function Button({
  variant = "default",
  size,
  loading = false,
  icon,
  className,
  onClick,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm"; loading?: boolean; icon?: ReactNode }) {
  const cls = ["btn", variant !== "default" && `btn--${variant}`, size && `btn--${size}`, className].filter(Boolean).join(" ");
  // Loading keeps the button focusable and its label; the click is ignored.
  const click = (e: MouseEvent<HTMLButtonElement>) => {
    if (loading) {
      e.preventDefault();
      return;
    }
    onClick?.(e);
  };
  return (
    <button type="button" className={cls} aria-busy={loading || undefined} onClick={click} {...rest}>
      {loading ? <Spinner /> : icon}
      {children}
    </button>
  );
}

/** Icon-only button: `label` is mandatory; it is the aria-label and the tooltip (shown on
 * hover and on keyboard focus, with `shortcut` as keycaps). `pressed` makes it a toggle
 * (aria-pressed, recessed when engaged). */
export function IconButton({
  label,
  shortcut,
  pressed,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; shortcut?: string; pressed?: boolean; children: ReactNode }) {
  return (
    <Tooltip label={label} shortcut={shortcut}>
      <button type="button" className="icon-btn" aria-label={label} aria-pressed={pressed} {...rest}>
        {children}
      </button>
    </Tooltip>
  );
}

// ---- Kbd, Keys, Badge, Count -----------------------------------------------------

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}

/** A chord as keycaps: `keys="Ctrl Shift R"` (platform-aware). */
export function Keys({ keys }: { keys: string }) {
  return (
    <kbd className="keys">
      {keyLabels(keys).map((k, i) => (
        <kbd key={i} className="kbd">
          {k}
        </kbd>
      ))}
    </kbd>
  );
}

export function Badge({ variant = "neutral", children }: { variant?: "neutral" | "accent"; children: ReactNode }) {
  return (
    <span className="badge" data-variant={variant}>
      {children}
    </span>
  );
}

export function Count({ value, label }: { value: number; label?: string }) {
  return (
    <span className="count" aria-label={label}>
      {value}
    </span>
  );
}

// ---- Led, Meter, RateMeter -------------------------------------------------------

export type LedState = "off" | "on" | "run" | "warn" | "err";

/** A status light. With `label` it is an image with that name; without, it is decorative
 * and must sit next to text that says the same thing. */
export function Led({ state, label }: { state: LedState; label?: string }) {
  return label ? (
    <span className="led" data-state={state} role="img" aria-label={label} />
  ) : (
    <span className="led" data-state={state} aria-hidden="true" />
  );
}

/** Determinate meter, value 0–1. */
export function Meter({ value, label }: { value: number; label: string }) {
  const v = Math.max(0, Math.min(1, value));
  return (
    <span className="meter" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v * 100)}>
      <span className="meter__fill" style={{ transform: `scaleX(${v})` }} />
    </span>
  );
}

/** Rate meter: 16 levels 0–1 as 2x14 px bars (no tweening). */
export function RateMeter({ levels, label }: { levels: number[]; label: string }) {
  const bars = Array.from({ length: 16 }, (_, i) => Math.max(0, Math.min(1, levels[i] ?? 0)));
  return (
    <span className="rate-meter" role="img" aria-label={label}>
      {bars.map((l, i) => (
        <span key={i} className="rate-meter__bar">
          <span style={{ transform: `scaleY(${l})` }} />
        </span>
      ))}
    </span>
  );
}

// ---- Field, Switch, Segmented, Stepper --------------------------------------------

/** Text input (recessed). `invalid` sets aria-invalid; `describedBy` points at the message. */
export function Field({
  mono = false,
  invalid = false,
  describedBy,
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement> & { mono?: boolean; invalid?: boolean; describedBy?: string }) {
  const cls = ["input", mono && "input--mono", className].filter(Boolean).join(" ");
  return <input className={cls} aria-invalid={invalid || undefined} aria-describedby={describedBy} {...rest} />;
}

/** On/off setting: role="switch"; click, Space and Enter toggle. */
export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className="switch"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      onKeyDown={(e: KeyboardEvent<HTMLButtonElement>) => {
        if (!isToggleKey(e.key)) return;
        // Handled here; preventing the default stops the button's own key activation
        // (Enter on keydown, Space on keyup) from toggling a second time.
        e.preventDefault();
        onChange(!checked);
      }}
      onKeyUp={(e: KeyboardEvent<HTMLButtonElement>) => {
        if (e.key === " ") e.preventDefault();
      }}
    >
      <span className="switch__thumb" />
    </button>
  );
}

/** 2–4 exclusive options: radiogroup with a gliding thumb; arrow keys move and select. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
  disabled,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
  disabled?: boolean;
}) {
  const index = Math.max(0, options.findIndex((o) => o.value === value));
  const style = { "--seg-n": options.length, "--seg-i": index } as CSSProperties;
  return (
    <div className="segmented" role="radiogroup" aria-label={label} aria-disabled={disabled || undefined} style={style}>
      <span className="segmented__glide" aria-hidden="true" />
      {options.map((o, i) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          tabIndex={i === index ? 0 : -1}
          disabled={disabled}
          onClick={() => onChange(o.value)}
          onKeyDown={(e: KeyboardEvent<HTMLButtonElement>) => {
            const next = segmentedTarget(e.key, i, options.length);
            if (next === null) return;
            e.preventDefault();
            onChange(options[next]!.value);
            const radios = e.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="radio"]');
            radios?.[next]?.focus();
          }}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Numeric setting: − field +; Up/Down step, typing commits integers within range. */
export function Stepper({
  value,
  min,
  max,
  step = 1,
  onChange,
  label,
  id,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  label: string;
  id?: string;
}) {
  const clamp = (v: number) => Math.max(min, Math.min(max, v));
  return (
    <span className="stepper">
      <Button size="sm" aria-label={`Decrease ${label}`} disabled={value <= min} onClick={() => onChange(clamp(value - step))}>
        −
      </Button>
      <input
        id={id}
        className="input"
        inputMode="numeric"
        aria-label={label}
        value={value}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (Number.isInteger(v)) onChange(clamp(v));
        }}
        onKeyDown={(e) => {
          const next = stepperTarget(e.key, value, { min, max, step });
          if (next === null) return;
          e.preventDefault();
          onChange(next);
        }}
      />
      <Button size="sm" aria-label={`Increase ${label}`} disabled={value >= max} onClick={() => onChange(clamp(value + step))}>
        +
      </Button>
    </span>
  );
}

// ---- unchanged in SYSTEM-003 (SYSTEM-004 / SHELL-002) -----------------------------

// Dialog, Sheet, Menu and Banner live in overlays.tsx (SYSTEM-004); re-exported here for
// the existing imports until M1.6-QA-006.
export { Banner, Dialog, Menu, Sheet, type DialogButton, type MenuEntry } from "./overlays";

// Splitter moved to ui/panel.tsx (SHELL-002).
