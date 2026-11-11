import { useEffect, useState, useSyncExternalStore } from "react";
import type { DocumentsStore } from "../state/documents";
import type { SettingsStore, ThemeSetting } from "../state/settings";
import type { AppInfo } from "../daemon/tauri-transport";
import { Dialog, Segmented, Sheet } from "../ui/primitives";

export function ConflictDialog({ documents }: { documents: DocumentsStore }) {
  const s = useSyncExternalStore(documents.subscribe, documents.getSnapshot);
  return (
    <Dialog
      open={s.conflictName !== null}
      testId="conflict-dialog"
      title={`${s.conflictName ?? ""} changed on disk`}
      description="Someone else changed this file after you opened it. Overwrite replaces their version with yours; Reload discards your changes and shows theirs."
      dismissAction="cancel"
      buttons={[
        { text: "Cancel", variant: "quiet", action: "cancel" },
        { text: "Reload", action: "reload" },
        { text: "Overwrite", variant: "primary", action: "overwrite" },
      ]}
      onChoose={(a) => documents.resolveConflict(a as "overwrite" | "reload" | "cancel")}
    />
  );
}

export function UnsavedDialog({ documents }: { documents: DocumentsStore }) {
  const s = useSyncExternalStore(documents.subscribe, documents.getSnapshot);
  const p = s.prompt;
  return (
    <Dialog
      open={p !== null}
      testId="unsaved-dialog"
      title={p?.kind === "close" ? `Save changes to ${p.names[0] ?? ""}?` : "Save changes before closing?"}
      description={p?.kind === "close" ? "Your changes will be lost if you do not save them." : "These documents have unsaved changes:"}
      dismissAction="cancel"
      buttons={[
        { text: "Cancel", variant: "quiet", action: "cancel" },
        { text: "Discard", action: "discard" },
        { text: "Save", variant: "primary", action: "save" },
      ]}
      onChoose={(a) => documents.resolvePrompt(a as "save" | "discard" | "cancel")}
    >
      {p && p.kind !== "close" && (
        <ul>
          {p.names.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}

export function SettingsSheet({ settings, open, onOpenChange }: { settings: SettingsStore; open: boolean; onOpenChange: (o: boolean) => void }) {
  const s = useSyncExternalStore(settings.subscribe, settings.getSnapshot);
  const [family, setFamily] = useState(s.editorFontFamily);
  useEffect(() => setFamily(s.editorFontFamily), [s.editorFontFamily]);
  return (
    <Sheet open={open} onOpenChange={onOpenChange} title="Settings">
      <div className="form-row">
        <label id="theme-label">Theme</label>
        <Segmented<ThemeSetting>
          label="Theme"
          value={s.theme}
          onChange={(v) => void settings.setValue("appearance.theme", v)}
          options={[
            { value: "system", label: "System" },
            { value: "light", label: "Light" },
            { value: "dark", label: "Dark" },
          ]}
        />
      </div>
      <div className="form-row">
        <label htmlFor="font-size">Editor font size</label>
        <input
          id="font-size"
          className="input"
          type="number"
          min={8}
          max={40}
          value={s.editorFontSize}
          onChange={(e) => {
            const v = Number(e.target.value);
            if (Number.isInteger(v)) void settings.setValue("editor.fontSize", v);
          }}
        />
      </div>
      <div className="form-row">
        <label htmlFor="font-family">Editor font</label>
        <input
          id="font-family"
          className="input"
          placeholder="Platform monospace"
          value={family}
          onChange={(e) => setFamily(e.target.value)}
          onBlur={() => family !== s.editorFontFamily && void settings.setValue("editor.fontFamily", family)}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
        />
      </div>
      <div className="form-row">
        <label htmlFor="wrap">Word wrap</label>
        <input id="wrap" type="checkbox" checked={s.editorWordWrap} onChange={(e) => void settings.setValue("editor.wordWrap", e.target.checked)} />
      </div>
      <div className="form-row">
        <label htmlFor="reopen">Reopen last folder</label>
        <input id="reopen" type="checkbox" checked={s.reopenLast} onChange={(e) => void settings.setValue("workspace.reopenLast", e.target.checked)} />
      </div>
    </Sheet>
  );
}

export function AboutSheet({
  open,
  onOpenChange,
  info,
  serverVersion,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  info: AppInfo | null;
  serverVersion: string | null;
}) {
  return (
    <Sheet open={open} onOpenChange={onOpenChange} title={`Axiom ${info?.appVersion ?? ""}`}>
      <dl className="about-grid">
        <dt>Backend</dt>
        <dd>{serverVersion ? `axiomd ${serverVersion}` : "not connected"}</dd>
        <dt>Protocol</dt>
        <dd>{info?.protocolVersion ?? "0.1"}</dd>
        <dt>Tauri</dt>
        <dd>{info?.tauriVersion ?? "—"}</dd>
        <dt>Webview</dt>
        <dd>{info?.webviewVersion ?? "—"}</dd>
        <dt>Data directory</dt>
        <dd title={info?.dataDir ?? undefined}>{info?.dataDir ?? "default location"}</dd>
      </dl>
    </Sheet>
  );
}
