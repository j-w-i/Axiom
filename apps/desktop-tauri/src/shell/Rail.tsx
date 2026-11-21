// The activity rail (D4, M2-UI-001; design-system.md ActivityRail): the top-level
// destinations (Files, Chat — M3, ADR-0017 — and Environments); every item does something. A `nav` whose current item carries aria-current="page"; ↑/↓ move
// between items (roving tabindex), Enter/Space activate.

import { useRef, useSyncExternalStore, type KeyboardEvent, type ReactNode } from "react";
import * as Icon from "../ui/icons";
import { Tooltip } from "../ui/overlays";
import "../styles/components/rail.css";

export type Destination = "files" | "chat" | "environments";

interface Item {
  id: Destination;
  label: string;
  icon: ReactNode;
  /** Attention count (e.g. toolchains missing); 0 = no badge. */
  badge?: number;
}

/** Where ↑/↓/Home/End move from `index` among `count` items; null = not a move key. */
export function railTarget(key: string, index: number, count: number): number | null {
  switch (key) {
    case "ArrowDown":
      return (index + 1) % count;
    case "ArrowUp":
      return (index - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** The badge count is subscribed here, so only the rail re-renders when detection finishes. */
export function Rail({
  current,
  onSelect,
  attention,
  subscribe,
  chatAttention = () => 0,
  chatSubscribe = () => () => {},
}: {
  current: Destination;
  onSelect: (d: Destination) => void;
  attention: () => number;
  subscribe: (fn: () => void) => () => void;
  /** Approvals waiting (M3-UI-002). */
  chatAttention?: () => number;
  chatSubscribe?: (fn: () => void) => () => void;
}) {
  const missing = useSyncExternalStore(subscribe, attention, attention);
  const waiting = useSyncExternalStore(chatSubscribe, chatAttention, chatAttention);
  const items: Item[] = [
    { id: "files", label: "Files", icon: <Icon.Files /> },
    { id: "chat", label: "Chat", icon: <Icon.Chat />, badge: waiting },
    { id: "environments", label: "Environments", icon: <Icon.Layers />, badge: missing },
  ];
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const to = railTarget(e.key, i, items.length);
    if (to === null) return;
    e.preventDefault();
    refs.current[to]?.focus();
  };
  return (
    <nav className="rail" aria-label="Activity" data-region="rail" data-testid="rail">
      {items.map((it, i) => {
        const active = it.id === current;
        const name = it.badge ? `${it.label}, ${it.badge} need attention` : it.label;
        return (
          <Tooltip key={it.id} label={it.label} side="right">
            <button
              ref={(el) => {
                refs.current[i] = el;
              }}
              type="button"
              className="rail__item"
              aria-label={name}
              aria-current={active ? "page" : undefined}
              tabIndex={active ? 0 : -1}
              data-destination={it.id}
              data-hint={it.label}
              onClick={() => onSelect(it.id)}
              onKeyDown={(e) => onKeyDown(e, i)}
            >
              {it.icon}
              {it.badge ? (
                <span className="rail__badge" aria-hidden="true">
                  {it.badge > 9 ? "9+" : it.badge}
                </span>
              ) : null}
            </button>
          </Tooltip>
        );
      })}
    </nav>
  );
}
