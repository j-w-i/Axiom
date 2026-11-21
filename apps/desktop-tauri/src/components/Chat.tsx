// Chat (M3-UI-001; ADR-0017; workspace-spec.md §5.4): a first-class, full-window conversation
// surface that works with or without an open folder. The side bar lists conversations; the main
// area is the transcript (Markdown, code, tool rows, streaming) and the composer. Everything is
// rendered from AgentStore; the webview never runs a model or a tool.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type ClipboardEvent, type DragEvent, type KeyboardEvent, type ReactNode } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ApprovalRequest, ImageData, ImageRef, SessionInfo } from "../daemon/protocol.gen";
import { filterSessions, lastUser, type AgentStore, type Item, type ProviderOption } from "../state/agent";
import * as Icon from "../ui/icons";
import { Dialog, Menu, type MenuEntry } from "../ui/overlays";
import { Button, Field, IconButton, Led, Segmented, Spinner } from "../ui/primitives";
import "../styles/components/chat.css";

const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
const IMAGE_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];
const TOOL_PREVIEW_LINES = 30;

/** "3 min", "2 h", "Yesterday", "12 Nov" from milliseconds. */
export function relativeTime(ms: number, now = Date.now()): string {
  const d = Math.max(0, now - ms);
  const min = Math.floor(d / 60_000);
  if (min < 1) return "now";
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} h`;
  if (h < 48) return "Yesterday";
  return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** ↑/↓/Home/End in the conversation list; null = not a move key. */
export function listTarget(key: string, index: number, count: number): number | null {
  if (count === 0) return null;
  switch (key) {
    case "ArrowDown":
      return Math.min(count - 1, index + 1);
    case "ArrowUp":
      return Math.max(0, index - 1);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** Reads a pasted, dropped or picked file as an image for `agent/send`, or a reason it is not. */
export function readImage(file: File): Promise<ImageData | string> {
  if (!IMAGE_TYPES.includes(file.type)) return Promise.resolve(`${file.name || "That file"} is not a PNG, JPEG, GIF or WebP image.`);
  if (file.size > MAX_IMAGE_BYTES) return Promise.resolve(`${file.name || "That image"} is larger than 5 MiB.`);
  return new Promise((resolve) => {
    const r = new FileReader();
    r.onload = () => {
      const url = String(r.result);
      resolve({ mediaType: file.type, data: url.slice(url.indexOf(",") + 1) });
    };
    r.onerror = () => resolve("The image could not be read.");
    r.readAsDataURL(file);
  });
}

// ---- side bar --------------------------------------------------------------------

export function ChatSide({ store, workspaceName }: { store: AgentStore; workspaceName: string }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const list = useMemo(() => filterSessions(s.sessions, s.search), [s.sessions, s.search]);
  const rows = useRef<(HTMLButtonElement | null)[]>([]);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<SessionInfo | null>(null);
  const activeIndex = Math.max(0, list.findIndex((x) => x.sessionId === s.activeId));
  const pending = (id: string) => s.approvals.some((a) => a.sessionId === id);
  const onKey = (e: KeyboardEvent<HTMLButtonElement>, i: number, row: SessionInfo) => {
    if (e.key === "F2") {
      e.preventDefault();
      setRenaming(row.sessionId);
      return;
    }
    if (e.key === "Delete") {
      e.preventDefault();
      setDeleting(row);
      return;
    }
    const to = listTarget(e.key, i, list.length);
    if (to === null) return;
    e.preventDefault();
    rows.current[to]?.focus();
  };
  return (
    <div className="chat-side" data-testid="chat-side">
      {workspaceName && (
        <div className="chat-side__scope">
          <Segmented
            label="Conversation scope"
            value={s.scope}
            options={[
              { value: "general", label: "General" },
              { value: "workspace", label: workspaceName },
            ]}
            onChange={(v) => store.setScope(v)}
          />
        </div>
      )}
      <div className="chat-side__search">
        <Field type="search" placeholder="Search conversations" aria-label="Search conversations" value={s.search} onChange={(e) => store.setSearch(e.target.value)} data-hint="Search conversations by title" />
      </div>
      <div className="chat-side__list" role="list" aria-label="Conversations">
        {list.length === 0 && <p className="chat-side__empty">{s.search ? "No conversation matches." : "No conversations yet."}</p>}
        {list.map((row, i) =>
          renaming === row.sessionId ? (
            <RenameRow key={row.sessionId} title={row.title} onDone={(t) => {
              setRenaming(null);
              if (t && t !== row.title) void store.rename(row.sessionId, t);
            }} />
          ) : (
            <div className="chat-row" role="listitem" key={row.sessionId}>
              <button
                ref={(el) => {
                  rows.current[i] = el;
                }}
                type="button"
                className="chat-row__open"
                aria-current={row.sessionId === s.activeId ? "true" : undefined}
                aria-label={`${row.title}${row.running ? ", replying" : ""}${pending(row.sessionId) ? ", waiting for your approval" : ""}`}
                tabIndex={i === activeIndex ? 0 : -1}
                data-session={row.sessionId}
                data-hint="Open — F2 renames, Delete deletes"
                title={row.title}
                onClick={() => void store.open(row.sessionId)}
                onKeyDown={(e) => onKey(e, i, row)}
              >
                {pending(row.sessionId) ? <Led state="warn" /> : row.running ? <Led state="run" /> : null}
                <span className="chat-row__title">{row.title}</span>
                <span className="chat-row__time">{relativeTime(row.updatedMs)}</span>
              </button>
              <Menu
                label={`More for ${row.title}`}
                triggerClassName="icon-btn chat-row__more"
                trigger={<Icon.More />}
                items={[
                  { label: "Rename", shortcut: "F2", onSelect: () => setRenaming(row.sessionId) },
                  { label: "Delete…", shortcut: "Delete", onSelect: () => setDeleting(row) },
                ]}
              />
            </div>
          ),
        )}
      </div>
      <Dialog
        open={deleting !== null}
        title={`Delete “${deleting?.title ?? ""}”?`}
        description="Its history is removed. Checkpoints of files it changed stay with the folder."
        buttons={[
          { action: "cancel", text: "Cancel", variant: "primary" },
          { action: "delete", text: "Delete", variant: "danger" },
        ]}
        dismissAction="cancel"
        testId="chat-delete"
        onChoose={(a) => {
          if (a === "delete" && deleting) void store.remove(deleting.sessionId);
          setDeleting(null);
        }}
      />
    </div>
  );
}

function RenameRow({ title, onDone }: { title: string; onDone: (t: string | null) => void }) {
  const [v, setV] = useState(title);
  return (
    <div className="chat-row chat-row--editing" role="listitem">
      <Field
        autoFocus
        aria-label="Conversation title"
        value={v}
        maxLength={120}
        onChange={(e) => setV(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") onDone(v.trim() || null);
          if (e.key === "Escape") onDone(null);
        }}
        onBlur={() => onDone(v.trim() || null)}
      />
    </div>
  );
}

// ---- main ------------------------------------------------------------------------

export function ChatPage({
  store,
  hasWorkspace,
  dark,
  onOpenLink,
  onOpenDiff,
  setup,
  variant = "page",
}: {
  store: AgentStore;
  hasWorkspace: boolean;
  dark: boolean;
  onOpenLink: (url: string) => void;
  /** Opens a workspace file's diff in the editor (M4-GIT-002); absent without a folder. */
  onOpenDiff?: (path: string) => void;
  /** What to show when no model is available (the first-run setup, M3-UI-003). */
  setup?: ReactNode;
  /** "dock": beside the editor (workspace-spec.md §5.2): the header switches conversations. */
  variant?: "page" | "dock";
}) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  useEffect(() => {
    if (s.status === "idle") void store.load();
  }, [store, s.status]);
  const items = useMemo(() => store.items(), [store, s.active.events]); // eslint-disable-line react-hooks/exhaustive-deps
  const title = s.sessions.find((x) => x.sessionId === s.activeId)?.title ?? "New conversation";
  const usable = s.providers.filter((p) => !p.error && p.models.length > 0);
  const noModel = s.status === "ready" && usable.length === 0;
  const modelOf = s.model ? s.providers.find((p) => p.providerId === s.model?.providerId) : undefined;
  const modelName = modelOf?.models.find((m) => m.id === s.model?.model)?.name ?? s.model?.model ?? "Choose a model";
  const imagesOk = modelOf?.models.find((m) => m.id === s.model?.model)?.images ?? false;
  const [announce, setAnnounce] = useState("");
  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !s.active.running) setAnnounce("Answer complete");
    wasRunning.current = s.active.running;
  }, [s.active.running]);
  const locked = s.activeId !== null; // a conversation keeps its model
  return (
    <section className={variant === "dock" ? "chat chat--dock" : "chat"} aria-label={variant === "dock" ? "Agent" : "Chat"} data-testid={variant === "dock" ? "chat-dock" : "chat-page"}>
      <header className="chat__head">
        {variant === "dock" ? (
          <Menu
            label={`Conversation: ${title}`}
            triggerClassName="btn btn--quiet btn--sm chat__switch"
            trigger={<span className="chat__title">{title}</span>}
            hint="Switch conversations in this folder"
            items={[
              { label: "New conversation", onSelect: () => store.newChat("workspace") },
              ...(s.sessions.length ? [{ separator: true } as MenuEntry] : []),
              ...s.sessions.slice(0, 20).map((x) => ({ label: x.title, onSelect: () => void store.open(x.sessionId) })),
            ]}
          />
        ) : (
          <h2 className="chat__title" title={title}>
            {title}
          </h2>
        )}
        <ModelPicker
          providers={s.providers}
          scopeWorkspace={s.scope === "workspace"}
          label={locked ? `Model: ${modelOf?.name ?? ""} · ${modelName}` : `Model: ${modelName}`}
          text={`${modelOf?.name ?? ""}${modelOf ? " · " : ""}${modelName}`}
          disabled={locked}
          onChoose={(c) => store.setModel(c)}
        />
        <IconButton label="New chat" shortcut="Mod+N" onClick={() => store.newChat()}>
          <Icon.Plus />
        </IconButton>
      </header>
      {s.error && (
        <p className="chat__error" role="alert">
          {s.error} <Button size="sm" variant="quiet" onClick={() => store.dismissError()}>Dismiss</Button>
        </p>
      )}
      {noModel && setup ? (
        <div className="chat__setup">{setup}</div>
      ) : (
        <Transcript store={store} items={items} streaming={s.active.streaming} running={s.active.running} images={s.images} dark={dark} onOpenLink={onOpenLink} onOpenDiff={onOpenDiff} hasWorkspace={hasWorkspace} empty={noModel ? "No model is available. Add one in Settings → Models." : null}
          approvals={s.approvals.filter((a) => a.sessionId === s.activeId)} external={modelOf?.kind === "acp" ? modelOf.name : null} />
      )}
      <Dialog
        open={s.restoreConflict !== null}
        title={(s.restoreConflict?.conflicts.length ?? 0) === 1 ? "1 file changed since the agent wrote it" : `${s.restoreConflict?.conflicts.length ?? 0} files changed since the agent wrote them`}
        description={
          <>
            Restoring checkpoint {s.restoreConflict?.checkpoint} would overwrite them, so nothing was restored. You can restore the other files and leave these as they are:
            <ul className="chat__conflicts">
              {s.restoreConflict?.conflicts.map((c) => (
                <li key={c.path}>
                  <code>{c.path}</code> — {c.reason === "modifiedSinceAgent" ? "edited" : c.reason === "deletedSinceAgent" ? "deleted" : "created"} after the agent
                </li>
              ))}
            </ul>
          </>
        }
        buttons={[
          { action: "cancel", text: "Cancel", variant: "primary" },
          { action: "rest", text: "Restore the others" },
        ]}
        dismissAction="cancel"
        testId="restore-conflict"
        onChoose={(a) => {
          const rc = s.restoreConflict;
          if (a === "rest" && rc) void store.restore(rc.checkpoint, true);
          else store.dismissRestoreConflict();
        }}
      />
      <p className="visually-hidden" role="status" aria-live="polite">
        {announce}
      </p>
      <Composer
        conversation={s.activeId}
        disabled={noModel || !s.model}
        running={s.active.running}
        modelName={modelName}
        imagesOk={imagesOk}
        error={s.sendError}
        onSend={(text, images) => store.send(text, images)}
        onStop={() => store.stop()}
        onEditLast={() => {
          const u = lastUser(items);
          if (u) document.querySelector<HTMLButtonElement>(`[data-edit-turn="${u.turn}"]`)?.click();
        }}
      />
    </section>
  );
}

function ModelPicker({
  providers,
  scopeWorkspace,
  label,
  text,
  disabled,
  onChoose,
}: {
  providers: ProviderOption[];
  scopeWorkspace: boolean;
  label: string;
  text: string;
  disabled: boolean;
  onChoose: (c: { providerId: string; model: string }) => void;
}) {
  if (disabled) {
    return (
      <span className="chat__model chat__model--locked" aria-label={label} data-hint="A conversation keeps its model; start a new chat to change it">
        {text}
      </span>
    );
  }
  const items: MenuEntry[] = [];
  for (const p of providers) {
    const agent = p.kind === "acp";
    items.push({ heading: agent ? `${p.name} (agent)` : p.name });
    if (p.error || p.models.length === 0) {
      items.push({ label: p.error ?? "No models", disabled: true, onSelect: () => {} });
      continue;
    }
    if (agent && !scopeWorkspace) {
      items.push({ label: "Works in a folder: choose the folder scope", disabled: true, onSelect: () => {} });
      continue;
    }
    for (const m of p.models) items.push({ label: m.name, onSelect: () => onChoose({ providerId: p.providerId, model: m.id }) });
  }
  return <Menu label={label} triggerClassName="btn btn--quiet btn--sm chat__model" trigger={<span>{text}</span>} items={items} hint="Choose the model for a new chat" />;
}

// ---- transcript ------------------------------------------------------------------

function Transcript({
  onOpenDiff,
  store,
  items,
  streaming,
  running,
  images,
  dark,
  onOpenLink,
  hasWorkspace,
  empty,
  approvals,
  external,
}: {
  store: AgentStore;
  items: Item[];
  streaming: string;
  running: boolean;
  images: Record<string, string>;
  dark: boolean;
  onOpenLink: (url: string) => void;
  onOpenDiff?: (path: string) => void;
  hasWorkspace: boolean;
  empty: string | null;
  approvals: ApprovalRequest[];
  /** The external agent's name when the conversation is with one (ADR-0016). */
  external: string | null;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [following, setFollowing] = useState(true);
  const lastAssistant = useMemo(() => {
    for (let i = items.length - 1; i >= 0; i--) if (items[i]?.kind === "assistant" && !(items[i] as { textKind?: string }).textKind) return items[i]?.key;
    return undefined;
  }, [items]);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && following) el.scrollTop = el.scrollHeight;
  }, [items, streaming, following, approvals.length]);
  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    setFollowing(el.scrollHeight - el.scrollTop - el.clientHeight < 24);
  };
  if (items.length === 0 && !streaming) {
    return (
      <div className="chat__scroll" ref={scroller}>
        <div className="chat__empty">
          <h3 className="chat__empty-title">Start a conversation</h3>
          <p className="chat__empty-text">{empty ?? (hasWorkspace ? "Ask about anything. In the folder scope the agent can read, change and build your project." : "Ask about anything: writing, code, ideas.")}</p>
        </div>
      </div>
    );
  }
  return (
    <div className="chat__scroll" ref={scroller} onScroll={onScroll}>
      <div className="chat__log" role="log" aria-label="Conversation" aria-live="off" data-testid="chat-log">
        {items.map((it) => (
          <ItemView key={it.key} item={it} store={store} images={images} dark={dark} onOpenLink={onOpenLink} onOpenDiff={onOpenDiff} last={it.key === lastAssistant && !running} running={running} />
        ))}
        {streaming && (
          <article className="msg msg--assistant" aria-label="Assistant" data-streaming="true">
            <MarkdownView text={streaming} dark={dark} onOpenLink={onOpenLink} />
            <span className="msg__caret" aria-hidden="true" />
          </article>
        )}
        {approvals.map((a) => (
          <ApprovalCard key={a.requestId} request={a} external={external} onAnswer={(d) => void store.approve(a.requestId, d)} />
        ))}
        {running && !streaming && approvals.length === 0 && (
          <p className="chat__working">
            <Spinner /> Working…
          </p>
        )}
      </div>
      {!following && (
        <button type="button" className="btn btn--sm chat__jump" onClick={() => setFollowing(true)} data-hint="Scroll to the newest message">
          <Icon.ArrowDown /> Jump to latest
        </button>
      )}
    </div>
  );
}

function ItemView({ item, store, images, dark, onOpenLink, onOpenDiff, last, running }: { item: Item; store: AgentStore; images: Record<string, string>; dark: boolean; onOpenLink: (url: string) => void; onOpenDiff?: (path: string) => void; last: boolean; running: boolean }) {
  switch (item.kind) {
    case "user":
      return <UserMessage item={item} store={store} images={images} running={running} />;
    case "assistant":
      if (item.textKind) {
        return (
          <details className="msg msg--note">
            <summary>{item.textKind === "plan" ? "Plan" : "Thinking"}</summary>
            <MarkdownView text={item.text} dark={dark} onOpenLink={onOpenLink} />
          </details>
        );
      }
      return (
        <article className="msg msg--assistant" aria-label="Assistant">
          <MarkdownView text={item.text} dark={dark} onOpenLink={onOpenLink} />
          <div className="msg__actions">
            <CopyButton text={item.text} label="Copy answer" />
            {last && (
              <IconButton label="Regenerate" onClick={() => void store.regenerate()} data-testid="chat-regenerate">
                <Icon.Refresh />
              </IconButton>
            )}
          </div>
        </article>
      );
    case "tool":
      return <ToolRow item={item} onOpenDiff={onOpenDiff} />;
    case "checkpoint":
      return (
        <p className="chat__checkpoint" data-checkpoint={item.number}>
          <span>
            Checkpoint {item.number} · {item.label} · {new Date(item.createdMs).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
          </span>
          <Button size="sm" variant="quiet" disabled={running} onClick={() => void store.restore(item.number)} data-hint="Put the files the agent changed since this point back" data-testid={`restore-${item.number}`}>
            Restore
          </Button>
        </p>
      );
    case "error":
      return (
        <p className={item.notice ? "chat__notice" : "chat__fail"} role={item.notice ? undefined : "alert"}>
          {!item.notice && <Icon.Warning />}
          {item.message}
        </p>
      );
  }
}

function UserMessage({ item, store, images, running }: { item: Item & { kind: "user" }; store: AgentStore; images: Record<string, string>; running: boolean }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(item.text);
  useEffect(() => {
    for (const r of item.images) void store.loadImage(r);
  }, [item.images, store]);
  if (editing) {
    return (
      <article className="msg msg--user msg--editing" aria-label="You (editing)">
        <textarea className="input chat__edit" aria-label="Edit message" value={text} autoFocus rows={Math.min(8, text.split("\n").length + 1)} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => {
          if (e.key === "Escape") setEditing(false);
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            setEditing(false);
            void store.edit(item.turn, text);
          }
        }} />
        <div className="msg__actions msg__actions--edit">
          <Button size="sm" onClick={() => setEditing(false)}>Cancel</Button>
          <Button size="sm" variant="primary" onClick={() => {
            setEditing(false);
            void store.edit(item.turn, text);
          }}>
            Save and send
          </Button>
        </div>
      </article>
    );
  }
  return (
    <article className="msg msg--user" aria-label="You">
      {item.images.length > 0 && (
        <div className="msg__images">
          {item.images.map((r: ImageRef) => (images[r.sha256] ? <img key={r.sha256} src={images[r.sha256]} alt="Attached image" className="msg__thumb" /> : <span key={r.sha256} className="msg__thumb msg__thumb--missing">Image</span>))}
        </div>
      )}
      <p className="msg__text">{item.text}</p>
      <div className="msg__actions">
        <IconButton label="Edit message" disabled={running} onClick={() => setEditing(true)} data-edit-turn={item.turn}>
          <Icon.Edit />
        </IconButton>
      </div>
    </article>
  );
}

function outputText(output: unknown): string {
  if (output && typeof output === "object") {
    const o = output as Record<string, unknown>;
    if (typeof o.diff === "string") return o.diff;
    if (typeof o.text === "string") return o.text;
    if (typeof o.output === "string") return o.output;
    if (typeof o.message === "string") return `${o.message}${typeof o.hint === "string" ? `\n${o.hint}` : ""}`;
  }
  return output === null || output === undefined ? "" : JSON.stringify(output, null, 2);
}

function diffOf(output: unknown): string | null {
  if (output && typeof output === "object") {
    const o = output as Record<string, unknown>;
    if (typeof o.diff === "string" && o.diff.startsWith("---")) return o.diff;
    if (typeof o.text === "string" && o.text.startsWith("--- a/")) return o.text;
  }
  return null;
}

function ToolRow({ item, onOpenDiff }: { item: Item & { kind: "tool" }; onOpenDiff?: (path: string) => void }) {
  const diff = diffOf(item.output);
  // A finished edit shows its diff until the person closes it (the row first renders running).
  const [chosen, setOpen] = useState<boolean | null>(null);
  const open = chosen ?? (diff !== null && item.state === "ok");
  const [all, setAll] = useState(false);
  const stateName = item.state === "running" ? "running" : item.state === "ok" ? "done" : "failed";
  const out = outputText(item.output);
  const lines = out.split("\n");
  return (
    <div className="tool-row" data-state={item.state} data-tool={item.tool}>
      <button type="button" className="tool-row__head" aria-expanded={open} aria-label={`${item.tool} ${item.targets.join(", ")}, ${stateName}`} onClick={() => setOpen(!open)} data-hint="Show the tool's result">
        <span className="tool-row__state" aria-hidden="true">
          {item.state === "running" ? <Spinner /> : item.state === "ok" ? <Icon.Check /> : <Icon.Close />}
        </span>
        <span className="tool-row__name">{item.tool}</span>
        <span className="tool-row__targets">{item.targets.join(", ")}</span>
        <span className="tool-row__summary">{item.summary}</span>
      </button>
      {open && diff && <DiffBlock diff={diff} onOpen={onOpenDiff} />}
      {open && !diff && out && (
        <pre className="tool-row__out">
          {all || lines.length <= TOOL_PREVIEW_LINES ? out : lines.slice(0, TOOL_PREVIEW_LINES).join("\n")}
          {!all && lines.length > TOOL_PREVIEW_LINES && (
            <>
              {"\n"}
              <Button size="sm" variant="quiet" onClick={() => setAll(true)}>
                Show all {lines.length} lines
              </Button>
            </>
          )}
        </pre>
      )}
    </div>
  );
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [done, setDone] = useState(false);
  return (
    <IconButton
      label={done ? "Copied" : label}
      onClick={() => {
        void navigator.clipboard?.writeText(text).then(() => {
          setDone(true);
          setTimeout(() => setDone(false), 1500);
        });
      }}
    >
      {done ? <Icon.Check /> : <Icon.Copy />}
    </IconButton>
  );
}

// ---- approvals and diffs -------------------------------------------------------------

const EFFECT_NAMES: Record<string, string> = {
  read: "reads files",
  writeWorkspace: "changes files in the folder",
  execute: "runs a program",
  install: "installs a toolchain",
  network: "uses the network",
  writeOutside: "changes files outside the folder",
  delete: "deletes files",
  credentials: "uses credentials",
  deploy: "deploys",
};

/** The approval card (workspace-spec.md §5.2): the only brass card in the log. Focused when it
 * appears; Enter allows once, Esc denies. */
export function ApprovalCard({
  request,
  external,
  context,
  onAnswer,
}: {
  request: ApprovalRequest;
  /** The external agent asking, if any (its own tools are outside Axiom's gate; ADR-0016). */
  external: string | null;
  /** Where it comes from, when not the open conversation ("Claude Code over MCP"…). */
  context?: string;
  onAnswer: (d: "once" | "session" | "deny") => void;
}) {
  const card = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Focus moves to the card only when the person is not typing: an Enter meant for a text
    // field or the editor must never approve. The card is announced either way (role=alert).
    // An empty composer is someone waiting, not typing.
    const a = document.activeElement as HTMLElement | null;
    const emptyComposer = a instanceof HTMLTextAreaElement && a.classList.contains("composer__field") && a.value === "";
    const typing = !!a && !emptyComposer && (a.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName) || !!a.closest(".monaco-editor"));
    if (!typing) card.current?.querySelector<HTMLButtonElement>("[data-answer=once]")?.focus();
  }, []);
  const targets = request.targets.join(", ");
  return (
    <div
      ref={card}
      className="approval"
      role="alert"
      aria-label={`Approval needed: ${request.tool} on ${targets}`}
      data-testid="approval"
      data-request={request.requestId}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onAnswer("deny");
        }
      }}
    >
      <div className="approval__head">
        <Icon.Shield />
        <span className="approval__title">
          Allow <code>{request.tool}</code>
          {targets ? (
            <>
              {" "}on <code>{targets}</code>
            </>
          ) : null}
          ?
        </span>
      </div>
      <p className="approval__meta">
        It {EFFECT_NAMES[request.effect] ?? request.effect}
        {request.rule ? ` · rule: ${request.rule}` : " · the policy asks by default"}
        {request.checkpointPlanned ? " · a checkpoint is taken first" : ""}
        {context ? ` · ${context}` : ""}
      </p>
      {external && <p className="approval__meta">{external} is an external agent: what it does without asking Axiom is outside this check.</p>}
      {request.preview && request.preview.startsWith("---") ? <DiffBlock diff={request.preview} /> : request.argv ? <pre className="approval__argv">{request.argv.join(" ")}</pre> : null}
      <div className="approval__actions">
        <Button variant="primary" data-answer="once" onClick={() => onAnswer("once")} data-hint="Allow this call — Enter">
          Allow once
        </Button>
        {request.canRemember && (
          <Button data-answer="session" onClick={() => onAnswer("session")} data-hint="Allow this tool on these targets for the rest of the session">
            Allow for session
          </Button>
        )}
        <Button data-answer="deny" onClick={() => onAnswer("deny")} data-hint="Refuse — Esc">
          Deny
        </Button>
      </div>
    </div>
  );
}

interface DiffRow {
  kind: "add" | "del" | "ctx" | "hunk";
  text: string;
  old?: number;
  new?: number;
}

/** A unified diff as rows with line numbers (pure; exported for tests). */
export function parseDiff(diff: string): { file: string; added: number; removed: number; rows: DiffRow[] } {
  const rows: DiffRow[] = [];
  let file = "";
  let added = 0;
  let removed = 0;
  let o = 0;
  let n = 0;
  for (const line of diff.split("\n")) {
    if (line.startsWith("+++ ")) {
      file = line.slice(4).replace(/^b\//, "");
      continue;
    }
    if (line.startsWith("--- ")) {
      if (!file) file = line.slice(4).replace(/^a\//, "");
      continue;
    }
    const h = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
    if (h) {
      o = Number(h[1]);
      n = Number(h[2]);
      rows.push({ kind: "hunk", text: line });
      continue;
    }
    if (line.startsWith("+")) {
      rows.push({ kind: "add", text: line.slice(1), new: n++ });
      added++;
    } else if (line.startsWith("-")) {
      rows.push({ kind: "del", text: line.slice(1), old: o++ });
      removed++;
    } else if (line.startsWith(" ")) {
      rows.push({ kind: "ctx", text: line.slice(1), old: o++, new: n++ });
    }
  }
  return { file, added, removed, rows };
}

export function DiffBlock({ diff, onOpen }: { diff: string; onOpen?: (path: string) => void }) {
  const d = useMemo(() => parseDiff(diff), [diff]);
  return (
    <figure className="diff" aria-label={`Diff of ${d.file}: ${d.added} added, ${d.removed} removed`}>
      <figcaption className="diff__head">
        <span className="diff__file">{d.file}</span>
        <span className="diff__stat">
          <span className="diff__plus">+{d.added}</span> <span className="diff__minus">−{d.removed}</span>
        </span>
        {onOpen && d.file && (
          <Button size="sm" variant="quiet" onClick={() => onOpen(d.file)} data-hint="Show the file's changes against HEAD in the diff editor">
            Open in diff editor
          </Button>
        )}
      </figcaption>
      <div className="diff__body" role="table" aria-label="Changed lines">
        {d.rows.map((r, i) =>
          r.kind === "hunk" ? (
            <div key={i} className="diff__row diff__row--hunk" role="row">
              <span role="cell">{r.text}</span>
            </div>
          ) : (
            <div key={i} className={`diff__row diff__row--${r.kind}`} role="row">
              <span className="diff__num" role="cell">{r.old ?? ""}</span>
              <span className="diff__num" role="cell">{r.new ?? ""}</span>
              <span className="diff__sign" role="cell" aria-label={r.kind === "add" ? "added" : r.kind === "del" ? "removed" : "unchanged"}>
                {r.kind === "add" ? "+" : r.kind === "del" ? "−" : " "}
              </span>
              <span className="diff__text" role="cell">{r.text}</span>
            </div>
          ),
        )}
      </div>
    </figure>
  );
}

/** Approvals not shown in an open conversation: other conversations', and external callers'
 * (MCP clients). A stack at the window's bottom right. */
export function ApprovalsStack({ store, onShow }: { store: AgentStore; onShow: (sessionId: string) => void }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const others = useMemo(() => store.otherApprovals(), [store, s.approvals, s.activeId]); // eslint-disable-line react-hooks/exhaustive-deps
  if (others.length === 0) return null;
  return (
    <section className="approvals" aria-label="Approvals waiting">
      {others.slice(0, 3).map((a) => {
        const session = a.sessionId ? s.sessions.find((x) => x.sessionId === a.sessionId) : undefined;
        const from = a.principal.startsWith("mcp:") ? `asked by ${a.principal.slice(4)} over MCP` : session ? `in “${session.title}”` : `asked by ${a.principal}`;
        return (
          <div key={a.requestId} className="approvals__item">
            <ApprovalCard request={a} external={null} context={from} onAnswer={(d) => void store.approve(a.requestId, d)} />
            {a.sessionId && (
              <Button size="sm" variant="quiet" onClick={() => onShow(a.sessionId as string)}>
                Show the conversation
              </Button>
            )}
          </div>
        );
      })}
      {others.length > 3 && <p className="approvals__more">{others.length - 3} more waiting</p>}
    </section>
  );
}

// ---- Markdown ----------------------------------------------------------------------

/** Assistant Markdown (CommonMark + GFM). Raw HTML is never rendered (react-markdown's default);
 * links open in the system browser through the host; remote images are not loaded. */
export function MarkdownView({ text, dark, onOpenLink }: { text: string; dark: boolean; onOpenLink: (url: string) => void }) {
  return (
    <div className="md">
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => (
            <a
              href={href}
              onClick={(e) => {
                e.preventDefault();
                if (href && /^https?:\/\//i.test(href)) onOpenLink(href);
              }}
              data-hint={href ? `Open ${href} in the browser` : undefined}
            >
              {children}
            </a>
          ),
          img: ({ alt, src }) => <span className="md__img">[image: {alt || src}]</span>,
          pre: ({ children }) => <>{children}</>,
          code: ({ className, children }) => {
            const lang = /language-([\w+#-]+)/.exec(className ?? "")?.[1];
            const code = String(children ?? "");
            const block = lang !== undefined || code.includes("\n");
            return block ? <CodeBlock code={code.replace(/\n$/, "")} language={lang ?? ""} dark={dark} /> : <code className="md__code">{children}</code>;
          },
        }}
      >
        {text}
      </Markdown>
    </div>
  );
}

function CodeBlock({ code, language, dark }: { code: string; language: string; dark: boolean }) {
  const [html, setHtml] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      void import("../editor/surface")
        .then((m) => m.colorizeCode(code, language, dark))
        .then((h) => live && setHtml(h))
        .catch(() => {});
    }, 120); // coalesce while the block is still streaming
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [code, language, dark]);
  return (
    <figure className="code-block">
      <figcaption className="code-block__head">
        <span className="code-block__lang">{language || "text"}</span>
        <CopyButton text={code} label="Copy code" />
      </figcaption>
      {html ? <pre className="code-block__body" dangerouslySetInnerHTML={{ __html: html }} /> : <pre className="code-block__body">{code}</pre>}
    </figure>
  );
}

// ---- composer ----------------------------------------------------------------------

function Composer({
  conversation,
  disabled,
  running,
  modelName,
  imagesOk,
  error,
  onSend,
  onStop,
  onEditLast,
}: {
  /** The open conversation: switching to another one clears the draft (creating one keeps it). */
  conversation: string | null;
  disabled: boolean;
  running: boolean;
  modelName: string;
  imagesOk: boolean;
  error: string;
  onSend: (text: string, images: ImageData[]) => Promise<boolean>;
  onStop: () => void;
  onEditLast: () => void;
}) {
  const [text, setText] = useState("");
  const [images, setImages] = useState<ImageData[]>([]);
  const [problem, setProblem] = useState("");
  const area = useRef<HTMLTextAreaElement>(null);
  const picker = useRef<HTMLInputElement>(null);
  const add = async (files: FileList | File[]) => {
    for (const f of Array.from(files)) {
      const r = await readImage(f);
      if (typeof r === "string") setProblem(r);
      else setImages((x) => [...x, r]);
    }
  };
  const send = async () => {
    if (running || disabled || (!text.trim() && images.length === 0)) return;
    const t = text;
    const im = images;
    setText("");
    setImages([]);
    setProblem("");
    if (!(await onSend(t, im))) {
      setText(t);
      setImages(im);
    }
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void send();
    } else if (e.key === "Escape" && running) {
      e.preventDefault();
      onStop();
    } else if (e.key === "ArrowUp" && text === "" && images.length === 0) {
      e.preventDefault();
      onEditLast();
    }
  };
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(e.clipboardData.files);
    if (files.length) {
      e.preventDefault();
      void add(files);
    }
  };
  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    if (e.dataTransfer.files.length) {
      e.preventDefault();
      void add(e.dataTransfer.files);
    }
  };
  const shown = useRef(conversation);
  useEffect(() => {
    const prev = shown.current;
    shown.current = conversation;
    if (prev !== conversation && !(prev === null && conversation !== null)) {
      setText("");
      setImages([]);
      setProblem("");
    }
    area.current?.focus();
  }, [conversation]);
  // When the composer becomes usable (the first-run setup just finished and unmounted the
  // control that had focus), the keyboard continues here instead of at the page.
  const wasDisabled = useRef(disabled);
  useEffect(() => {
    const was = wasDisabled.current;
    wasDisabled.current = disabled;
    const lost = !document.activeElement || document.activeElement === document.body;
    if (was && !disabled && lost) area.current?.focus();
  }, [disabled]);
  const rows = Math.min(8, Math.max(1, text.split("\n").length));
  const note = problem || error || (images.length > 0 && !imagesOk ? `${modelName} cannot see images; it will be told one was attached.` : "");
  return (
    <div className="composer" onDrop={onDrop} onDragOver={(e) => e.preventDefault()} data-testid="composer">
      {note && (
        <p className="composer__note" role={error || problem ? "alert" : undefined}>
          {note}
        </p>
      )}
      {images.length > 0 && (
        <div className="composer__chips">
          {images.map((im, i) => (
            <span className="composer__chip" key={i}>
              <img src={`data:${im.mediaType};base64,${im.data}`} alt="" />
              <IconButton label="Remove image" onClick={() => setImages((x) => x.filter((_, j) => j !== i))}>
                <Icon.Close />
              </IconButton>
            </span>
          ))}
        </div>
      )}
      <div className="composer__box">
        <textarea
          ref={area}
          className="composer__field"
          aria-label="Message"
          placeholder={disabled ? "No model is available" : `Message ${modelName}…`}
          value={text}
          rows={rows}
          disabled={disabled}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
          data-hint="Enter sends · Shift+Enter new line · Esc stops · ↑ edits the last message"
        />
        <input ref={picker} type="file" accept={IMAGE_TYPES.join(",")} multiple hidden onChange={(e) => {
          if (e.target.files) void add(e.target.files);
          e.target.value = "";
        }} />
        <IconButton label="Attach images" disabled={disabled} onClick={() => picker.current?.click()}>
          <Icon.Image />
        </IconButton>
        {running ? (
          <IconButton label="Stop" shortcut="Esc" className="icon-btn composer__stop" onClick={onStop} data-testid="chat-stop">
            <Icon.Stop />
          </IconButton>
        ) : (
          <IconButton label="Send" shortcut="Enter" className="icon-btn composer__send" disabled={disabled || (!text.trim() && images.length === 0)} onClick={() => void send()} data-testid="chat-send">
            <Icon.Send />
          </IconButton>
        )}
      </div>
    </div>
  );
}
