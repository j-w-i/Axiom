// Editor bridge 1 — hand-written from docs/protocols/editor-bridge.md.
// Keep in sync with that document; it is the contract (frozen for M1).

export const BRIDGE_VERSION = 1;

export type Theme = "light" | "dark";
export type EditorCommand = "undo" | "redo" | "find" | "replace" | "selectAll" | "format";

// ---- Shell -> page (signals) payloads ----

export interface ConfigurePayload {
  theme: Theme;
  fontSize: number;
  fontFamily: string;
  wordWrap: boolean;
}
export interface OpenDocumentPayload {
  path: string;
  text: string;
  version: string;
  activate: boolean;
  dirty: boolean;
}
export interface PathPayload {
  path: string;
}
export interface DocumentSavedPayload {
  path: string;
  version: string;
}
export interface RevealPositionPayload {
  path: string;
  /** 1-based. */
  line: number;
  /** 1-based. */
  column: number;
}
export interface RunCommandPayload {
  command: EditorCommand;
}

// ---- Page -> shell (slots) payloads ----

export interface ReadyPayload {
  bridgeVersion: 1;
  monacoVersion: string;
}
export interface SaveRequestedPayload {
  path: string;
  text: string;
}
export interface DirtyChangedPayload {
  path: string;
  dirty: boolean;
}
export interface CursorChangedPayload {
  path: string;
  line: number;
  column: number;
  selectionLength: number;
}
export interface TextSnapshotPayload {
  path: string;
  text: string;
}
export interface OpenLinkPayload {
  url: string;
}
export interface ReportErrorPayload {
  message: string;
  detail: string;
}

/** A QWebChannel signal as seen from the page. */
export interface Signal<T> {
  connect(listener: (payload: T) => void): void;
}

/** Shell -> page messages. */
export interface HostSignals {
  configure: Signal<ConfigurePayload>;
  openDocument: Signal<OpenDocumentPayload>;
  activateDocument: Signal<PathPayload>;
  closeDocument: Signal<PathPayload>;
  requestSave: Signal<PathPayload>;
  documentSaved: Signal<DocumentSavedPayload>;
  saveFailed: Signal<PathPayload>;
  revealPosition: Signal<RevealPositionPayload>;
  runCommand: Signal<RunCommandPayload>;
}

/** Page -> shell messages. Every slot takes exactly one object argument. */
export interface HostSlots {
  ready(payload: ReadyPayload): void;
  saveRequested(payload: SaveRequestedPayload): void;
  dirtyChanged(payload: DirtyChangedPayload): void;
  cursorChanged(payload: CursorChangedPayload): void;
  textSnapshot(payload: TextSnapshotPayload): void;
  openLink(payload: OpenLinkPayload): void;
  reportError(payload: ReportErrorPayload): void;
}

/** The QWebChannel object named `host`. */
export type Host = HostSignals & HostSlots;
