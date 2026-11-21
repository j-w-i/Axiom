// Welcome (workspace-spec.md §6; M1.6-SHELL-007): the editor area with no workspace. A
// centred column with the mark, one line of context, Open Folder… and up to eight recent
// folders. While the backend is not ready the actions are disabled and say why.

import type { RecentWorkspace } from "../daemon/types";
import * as Icon from "../ui/icons";
import { Tooltip } from "../ui/overlays";
import { shortenHome } from "../ui/paths";
import { Button, Kbd } from "../ui/primitives";
import "../styles/components/welcome.css";

export const WAITING = "Waiting for the backend";

export function Welcome({
  recent,
  ready,
  onOpenFolder,
  onOpenRecent,
  onStartChat,
  mod,
}: {
  recent: RecentWorkspace[];
  ready: boolean;
  onOpenFolder: () => void;
  onOpenRecent: (root: string) => void;
  /** Chat needs no folder (ADR-0017). */
  onStartChat?: () => void;
  mod: string;
}) {
  // Not `disabled` while waiting: a disabled button cannot be hovered or focused, so its
  // tooltip could never explain why. aria-disabled keeps it reachable; the click is ignored.
  const open = (
    <Button
      variant="primary"
      aria-disabled={!ready || undefined}
      data-hint={ready ? "Open a folder — Mod+O" : WAITING}
      onClick={() => {
        if (ready) onOpenFolder();
      }}
      data-testid="welcome-open"
    >
      Open Folder…
      <span className="keys welcome__keys" aria-hidden="true">
        <Kbd>{mod}</Kbd>
        <Kbd>O</Kbd>
      </span>
    </Button>
  );
  return (
    <div className="welcome" data-testid="welcome">
      <div className="welcome__inner">
        <Icon.Logo className="welcome__mark" />
        <h1 className="welcome__title">Open a folder to start</h1>
        <p className="welcome__line">Axiom opens folders through its backend, axiomd.</p>
        <div className="welcome__actions">
          {ready ? open : <Tooltip label={WAITING}>{open}</Tooltip>}
          {onStartChat && (
            <Button aria-disabled={!ready || undefined} data-hint={ready ? "Talk to a model — no folder needed" : WAITING} onClick={() => ready && onStartChat()} data-testid="welcome-chat">
              Start a chat
            </Button>
          )}
        </div>
        {recent.length > 0 && (
          <section className="welcome__recent" aria-labelledby="welcome-recent">
            <h2 className="welcome__label" id="welcome-recent">
              Recent
            </h2>
            <ul className="welcome__list">
              {recent.slice(0, 8).map((r) => (
                <li key={r.root}>
                  <button
                    type="button"
                    className="welcome__item"
                    disabled={!ready}
                    onClick={() => onOpenRecent(r.root)}
                    title={r.root}
                    data-hint={ready ? `Open ${r.name}` : WAITING}
                  >
                    <span className="welcome__item-name">{r.name}</span>
                    <span className="welcome__item-path">{shortenHome(r.root)}</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </div>
  );
}
