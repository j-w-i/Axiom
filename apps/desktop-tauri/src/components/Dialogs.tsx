import * as RDialog from "@radix-ui/react-dialog";
import { useEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent, type ReactNode } from "react";
import type { DocumentsStore } from "../state/documents";
import type { SettingsStore, ThemeSetting } from "../state/settings";
import type { AppInfo } from "../daemon/tauri-transport";
import type { MotionLevel } from "../ui/motion";
import { useReturnFocus } from "../ui/overlays";
import { Button, Dialog, Field, Segmented, Stepper, Switch } from "../ui/primitives";
import "../styles/components/settings.css";

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

/** The settings sections (workspace-spec.md §4); no Agents (M3) or Environments (M2) yet. */
export const SETTINGS_SECTIONS = [
  { id: "appearance", label: "Appearance" },
  { id: "editor", label: "Editor" },
  { id: "workspace", label: "Workspace" },
  { id: "about", label: "About" },
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]["id"];

/** Where the project's licences are published (README "Licence": MIT or Apache-2.0). */
const LICENCES_URL = "https://github.com/j-w-i/Axiom#licence";

/** The code face's name for the font-family placeholder: the first family of --font-code. */
function codeFaceName(): string {
  if (typeof document === "undefined") return "IBM Plex Mono";
  const first = getComputedStyle(document.documentElement).getPropertyValue("--font-code").split(",")[0]?.trim().replace(/^["']|["']$/g, "");
  // "Axiom Plex Mono" is the bundled IBM Plex Mono under the app's own family name (fonts.css).
  return !first || first === "Axiom Plex Mono" ? "IBM Plex Mono" : first;
}

/**
 * Settings (workspace-spec.md §4; M1.6-SURFACES-002): a two-pane dialog, section tabs on the
 * left, setting rows on the right. Changes apply at once through settings.setValue; a rejected
 * value is reported under its row rather than as a banner. About is a section.
 */
export function SettingsDialog({
  settings,
  open,
  section: initial,
  onOpenChange,
  info,
  serverVersion,
  onRestartBackend,
  onOpenExternal,
}: {
  settings: SettingsStore;
  open: boolean;
  section: SettingsSection;
  onOpenChange: (o: boolean) => void;
  info: AppInfo | null;
  serverVersion: string | null;
  onRestartBackend: () => void;
  onOpenExternal: (url: string) => void;
}) {
  const s = useSyncExternalStore(settings.subscribe, settings.getSnapshot);
  const [section, setSection] = useState<SettingsSection>(initial);
  const [family, setFamily] = useState(s.editorFontFamily);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const body = useRef<HTMLDivElement>(null);
  const returnFocus = useReturnFocus(open);
  useEffect(() => setFamily(s.editorFontFamily), [s.editorFontFamily]);
  useEffect(() => {
    if (open) {
      setSection(initial);
      setErrors({});
    }
  }, [open, initial]);

  /** Applies a value; a rejection is shown under the row (and not as the store's banner). */
  const apply = async (key: string, value: unknown) => {
    await settings.setValue(key, value);
    const error = settings.getSnapshot().error;
    if (error) settings.dismissError();
    setErrors((e) => ({ ...e, [key]: error }));
  };

  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const onTabKey = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const n = SETTINGS_SECTIONS.length;
    const next = e.key === "ArrowDown" ? (i + 1) % n : e.key === "ArrowUp" ? (i - 1 + n) % n : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (next === null) return;
    e.preventDefault();
    setSection(SETTINGS_SECTIONS[next]!.id);
    tabs.current[next]?.focus();
  };

  const row = (key: string, label: string, description: string, control: ReactNode) => {
    const err = errors[key];
    const errId = `setting-error-${key.replace(/\W/g, "-")}`;
    return (
      <div className="setting" aria-describedby={err ? errId : undefined}>
        <div className="setting__text">
          <span className="setting__label">{label}</span>
          {description && <span className="setting__desc">{description}</span>}
          {err && (
            <span className="setting__error" id={errId} role="alert">
              {err}
            </span>
          )}
        </div>
        <div className="setting__control">{control}</div>
      </div>
    );
  };

  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="overlay" />
        <RDialog.Content
          className="settings"
          data-testid="settings"
          aria-describedby={undefined}
          onCloseAutoFocus={returnFocus}
          onOpenAutoFocus={(e) => {
            // Land on the section's first control (Appearance: the theme choice).
            e.preventDefault();
            requestAnimationFrame(() => body.current?.querySelector<HTMLElement>('[tabindex="0"], button:not([disabled]), input')?.focus());
          }}
        >
          <RDialog.Title className="visually-hidden">Settings</RDialog.Title>
          <div className="settings__nav" role="tablist" aria-orientation="vertical" aria-label="Settings sections">
            {SETTINGS_SECTIONS.map((t, i) => (
              <button
                key={t.id}
                ref={(el) => {
                  tabs.current[i] = el;
                }}
                type="button"
                role="tab"
                id={`settings-tab-${t.id}`}
                aria-controls={`settings-panel-${t.id}`}
                aria-selected={section === t.id}
                tabIndex={section === t.id ? 0 : -1}
                className="settings__tab"
                onClick={() => setSection(t.id)}
                onKeyDown={(e) => onTabKey(e, i)}
              >
                {t.label}
              </button>
            ))}
          </div>
          <div className="settings__main">
            <div className="settings__body" ref={body} role="tabpanel" id={`settings-panel-${section}`} aria-labelledby={`settings-tab-${section}`}>
              {section === "appearance" && (
                <>
                  <h2 className="settings__heading">Appearance</h2>
                  {row(
                    "appearance.theme",
                    "Theme",
                    "Follow the operating system, or always use light or dark.",
                    <Segmented<ThemeSetting>
                      label="Theme"
                      value={s.theme}
                      onChange={(v) => void apply("appearance.theme", v)}
                      options={[
                        { value: "system", label: "System" },
                        { value: "light", label: "Light" },
                        { value: "dark", label: "Dark" },
                      ]}
                    />,
                  )}
                  {row(
                    "appearance.motion",
                    "Motion",
                    "Reduced keeps fades under 100 ms. The system's reduced-motion setting always applies.",
                    <Segmented<MotionLevel>
                      label="Motion"
                      value={s.motion}
                      onChange={(v) => void apply("appearance.motion", v)}
                      options={[
                        { value: "standard", label: "Standard" },
                        { value: "reduced", label: "Reduced" },
                        { value: "off", label: "Off" },
                      ]}
                    />,
                  )}
                </>
              )}
              {section === "editor" && (
                <>
                  <h2 className="settings__heading">Editor</h2>
                  {row(
                    "editor.fontSize",
                    "Font size",
                    "In pixels, for the editor.",
                    <Stepper id="font-size" label="Font size" value={s.editorFontSize} min={8} max={32} onChange={(v) => void apply("editor.fontSize", v)} />,
                  )}
                  {row(
                    "editor.fontFamily",
                    "Font family",
                    "Empty uses the bundled code face.",
                    <Field
                      id="font-family"
                      aria-label="Font family"
                      className="settings__field"
                      placeholder={codeFaceName()}
                      value={family}
                      invalid={!!errors["editor.fontFamily"]}
                      onChange={(e) => setFamily(e.target.value)}
                      onBlur={() => family !== s.editorFontFamily && void apply("editor.fontFamily", family)}
                      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                    />,
                  )}
                  {row("editor.wordWrap", "Word wrap", "Wrap long lines at the edge of the editor.", <Switch label="Word wrap" checked={s.editorWordWrap} onChange={(v) => void apply("editor.wordWrap", v)} />)}
                </>
              )}
              {section === "workspace" && (
                <>
                  <h2 className="settings__heading">Workspace</h2>
                  {row(
                    "workspace.reopenLast",
                    "Reopen last workspace",
                    "On launch, open the last folder with its tabs and expanded directories.",
                    <Switch label="Reopen last workspace" checked={s.reopenLast} onChange={(v) => void apply("workspace.reopenLast", v)} />,
                  )}
                </>
              )}
              {section === "about" && (
                <>
                  <h2 className="settings__heading">About</h2>
                  <p className="settings__about-title">Axiom {info?.appVersion ?? ""}</p>
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
                  <div className="settings__about-actions">
                    <Button variant="quiet" onClick={() => onOpenExternal(LICENCES_URL)}>
                      Licences (MIT or Apache-2.0)
                    </Button>
                    <Button variant="quiet" onClick={onRestartBackend}>
                      Restart backend
                    </Button>
                  </div>
                </>
              )}
            </div>
            <div className="settings__foot">
              <RDialog.Close asChild>
                <Button variant="primary">Done</Button>
              </RDialog.Close>
            </div>
          </div>
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}
