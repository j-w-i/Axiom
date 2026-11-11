import type { RecentWorkspace } from "../daemon/types";
import * as Icon from "../ui/icons";
import { Button, Kbd } from "../ui/primitives";

export function Welcome({
  recent,
  ready,
  onOpenFolder,
  onOpenRecent,
  mod,
}: {
  recent: RecentWorkspace[];
  ready: boolean;
  onOpenFolder: () => void;
  onOpenRecent: (root: string) => void;
  mod: string;
}) {
  return (
    <div className="welcome" data-testid="welcome">
      <div className="welcome__inner">
        <div>
          <h1 className="welcome__title">Welcome to Axiom</h1>
          <p className="welcome__subtitle">Open a folder to browse, edit and run it. Everything goes through the Axiom backend.</p>
        </div>
        <div className="welcome__actions">
          <Button variant="primary" onClick={onOpenFolder} disabled={!ready}>
            <Icon.FolderOpen /> Open Folder… <Kbd>{mod}+O</Kbd>
          </Button>
        </div>
        {recent.length > 0 && (
          <div>
            <div className="welcome__label">Recent</div>
            <ul className="welcome__list">
              {recent.slice(0, 8).map((r) => (
                <li key={r.root}>
                  <button type="button" className="welcome__item" disabled={!ready} onClick={() => onOpenRecent(r.root)} title={r.root}>
                    <span className="welcome__item-name">{r.name}</span>
                    <span className="welcome__item-path">{r.root}</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
