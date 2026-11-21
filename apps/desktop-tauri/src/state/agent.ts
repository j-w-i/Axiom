// The Chat surface's state (M3-UI-001; ADR-0017; workspace-spec.md §5.4): providers and models,
// the conversations of the current scope, and the open conversation's transcript, built from the
// session's `agent/event` stream (replayed by `agent/sessionOpen`). Streamed text is coalesced
// per animation frame. Everything comes from `agent/*`; the UI never runs a model or a tool.

import type { DaemonClient } from "../daemon/client";
import type {
  AgentEvent,
  ApprovalRequest,
  ApprovalResolution,
  EventBody,
  ImageData,
  ImageRef,
  ModelsResult,
  ProviderKind,
  RestoreConflict,
  RestoreResult,
  SessionInfo,
  SessionListResult,
  SessionOpenResult,
  TextKind,
} from "../daemon/protocol.gen";
import { RpcError } from "../daemon/types";
import { Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export type Scope = "general" | "workspace";

export interface ModelOption {
  id: string;
  name: string;
  images: boolean;
}

export interface ProviderOption {
  providerId: string;
  name: string;
  kind: ProviderKind;
  models: ModelOption[];
  /** Why it cannot be used now (one sentence), if so. */
  error?: string;
}

export interface ModelChoice {
  providerId: string;
  model: string;
}

/** One thing in the transcript. */
export type Item =
  | { kind: "user"; key: string; turn: number; text: string; images: ImageRef[] }
  | { kind: "assistant"; key: string; turn: number; text: string; textKind?: TextKind | null }
  | {
      kind: "tool";
      key: string;
      turn: number;
      callId: string;
      tool: string;
      targets: string[];
      state: "running" | "ok" | "error";
      summary: string;
      output: unknown;
    }
  | { kind: "checkpoint"; key: string; turn: number; number: number; label: string; createdMs: number }
  | { kind: "error"; key: string; turn: number; message: string; notice: boolean };

export interface Conversation {
  events: AgentEvent[];
  /** Text streamed since the last complete block (not persisted). */
  streaming: string;
  running: boolean;
}

export interface AgentState {
  status: "idle" | "loading" | "ready" | "error";
  error: string;
  providers: ProviderOption[];
  scope: Scope;
  sessions: SessionInfo[];
  search: string;
  /** The open conversation; null = a new chat not yet created. */
  activeId: string | null;
  active: Conversation;
  /** What a new chat (and the next session created) uses. */
  model: ModelChoice | null;
  /** A send or a turn failed before it started (shown above the composer). */
  sendError: string;
  /** sha256 → data URL of images shown in the transcript. */
  images: Record<string, string>;
  /** Approvals waiting, by request id (M3-UI-002). */
  approvals: ApprovalRequest[];
  /** A restore refused because of conflicts, waiting for the person's choice. */
  restoreConflict: { checkpoint: number; conflicts: RestoreConflict[] } | null;
}

const EMPTY: Conversation = { events: [], streaming: "", running: false };
const MODEL_KEY = "axiom.chat.model";

const initial: AgentState = {
  status: "idle",
  error: "",
  providers: [],
  scope: "general",
  sessions: [],
  search: "",
  activeId: null,
  active: EMPTY,
  model: null,
  sendError: "",
  images: {},
  approvals: [],
  restoreConflict: null,
};

/** The transcript of a session's persisted events (rewinds applied). Pure. */
export function transcript(events: AgentEvent[]): Item[] {
  let items: Item[] = [];
  const tools = new Map<string, Item & { kind: "tool" }>();
  for (const e of events) {
    const b: EventBody = e.event;
    const key = `${e.seq}`;
    switch (b.type) {
      case "user":
        items.push({ kind: "user", key, turn: e.turn, text: b.text, images: b.images ?? [] });
        break;
      case "rewind":
        items = items.filter((i) => i.turn < b.fromTurn);
        break;
      case "assistantText":
        items.push({ kind: "assistant", key, turn: e.turn, text: b.text, textKind: b.kind });
        break;
      case "toolCall": {
        const t: Item & { kind: "tool" } = {
          kind: "tool",
          key,
          turn: e.turn,
          callId: b.callId,
          tool: b.tool,
          targets: b.targets ?? [],
          state: "running",
          summary: "",
          output: null,
        };
        tools.set(b.callId, t);
        items.push(t);
        break;
      }
      case "toolResult": {
        const t = tools.get(b.callId);
        if (t) {
          const done = { ...t, state: b.isError ? ("error" as const) : ("ok" as const), summary: b.summary, output: b.output };
          items = items.map((i) => (i === t ? done : i));
          tools.set(b.callId, done);
        }
        break;
      }
      case "interrupted": {
        const t = tools.get(b.callId);
        if (t) {
          const done = { ...t, state: "error" as const, summary: "Interrupted: Axiom stopped while it ran" };
          items = items.map((i) => (i === t ? done : i));
          tools.set(b.callId, done);
        }
        break;
      }
      case "checkpoint":
        items.push({ kind: "checkpoint", key, turn: e.turn, number: b.checkpoint.number, label: b.checkpoint.label, createdMs: b.checkpoint.createdMs });
        break;
      case "error":
        items.push({ kind: "error", key, turn: e.turn, message: b.message, notice: b.kind === "notice" });
        break;
      case "restored": {
        const n = b.result.restored.length;
        const skipped = b.result.conflicts?.length ?? 0;
        const message = `Restored checkpoint ${b.result.checkpoint}: ${n} file${n === 1 ? "" : "s"}${skipped ? `, ${skipped} left as they were` : ""}. Checkpoint ${b.result.undoCheckpoint} undoes this.`;
        items.push({ kind: "error", key, turn: e.turn, message, notice: true });
        break;
      }
      default:
        break; // usage, turnEnd, compaction, approvals: not shown as items
    }
  }
  return items;
}

/** The last user message (for ↑ to edit and for Regenerate). */
export function lastUser(items: Item[]): (Item & { kind: "user" }) | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it?.kind === "user") return it;
  }
  return null;
}

/** Sessions matching the search (title, case-insensitive). Pure. */
export function filterSessions(sessions: SessionInfo[], search: string): SessionInfo[] {
  const q = search.trim().toLowerCase();
  return q ? sessions.filter((s) => s.title.toLowerCase().includes(q)) : sessions;
}

/** The provider options from `agent/models`. Pure. */
export function providerOptions(r: ModelsResult): ProviderOption[] {
  return r.providers.map((p) => ({
    providerId: p.config.id,
    name: p.config.name,
    kind: p.config.kind,
    models: p.models.map((m) => ({ id: m.id, name: m.displayName, images: m.capabilities.images !== "no" })),
    error: p.error ?? undefined,
  }));
}

/** An agent/* failure as one sentence. */
export function describeAgentError(e: unknown): string {
  if (!(e instanceof RpcError)) return String(e);
  switch (e.kind) {
    case "conflict":
      return "A reply is still being written. Stop it first.";
    case "model_error":
      return e.message;
    default:
      return e.message;
  }
}

/** Requests one animation frame (or a timer where there is no window, as in tests). */
const frame = (fn: () => void): void => {
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => fn());
  else setTimeout(fn, 16);
};

export class AgentStore extends Observable<AgentState> {
  /** Conversations seen since launch (they keep updating while another is open). */
  private readonly cache = new Map<string, Conversation>();
  private pendingDelta = new Map<string, string>();
  private flushScheduled = false;
  private loadSeq = 0;
  /** Where the chosen model is remembered (tests set null). */
  storage: Pick<Storage, "getItem" | "setItem"> | null = typeof localStorage === "undefined" ? null : localStorage;

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super(initial);
    client.onNotification<AgentEvent>("agent/event", (e) => this.onEvent(e));
    client.onNotification<ApprovalRequest>("agent/approvalRequested", (r) =>
      this.set({ approvals: [...this.state.approvals.filter((a) => a.requestId !== r.requestId), r] }),
    );
    client.onNotification<ApprovalResolution>("agent/approvalResolved", (r) =>
      this.set({ approvals: this.state.approvals.filter((a) => a.requestId !== r.requestId) }),
    );
    let lastWs: string | null = null;
    workspace.subscribe(() => {
      const id = workspace.getSnapshot().id || null;
      if (id === lastWs) return;
      lastWs = id;
      // Closing the folder leaves its scope.
      if (!id && this.state.scope === "workspace") this.newChat("general");
      else if (this.state.status !== "idle") void this.refreshSessions();
    });
    try {
      const raw = this.storage?.getItem(MODEL_KEY);
      if (raw) this.state = { ...this.state, model: JSON.parse(raw) as ModelChoice };
    } catch {
      /* a stored value from an older build: ignore */
    }
  }

  private get wsId(): string | undefined {
    return this.state.scope === "workspace" ? this.workspace.getSnapshot().id || undefined : undefined;
  }

  /** Loads providers and the conversation list (first open of Chat, or after a change). */
  async load(): Promise<void> {
    this.set({ status: this.state.status === "ready" ? "ready" : "loading", error: "" });
    try {
      const m = await this.client.request<ModelsResult>("agent/models", {});
      const providers = providerOptions(m);
      let model = this.state.model;
      const usable = (c: ModelChoice | null) =>
        !!c && providers.some((p) => p.providerId === c.providerId && !p.error && p.models.some((x) => x.id === c.model));
      if (!usable(model)) {
        const first = providers.find((p) => !p.error && p.models.length && (p.kind !== "acp" || this.state.scope === "workspace"));
        const m0 = first?.models[0];
        model = first && m0 ? { providerId: first.providerId, model: m0.id } : null;
      }
      this.set({ providers, model, status: "ready" });
      await this.refreshSessions();
    } catch (e) {
      this.set({ status: "error", error: describeAgentError(e) });
    }
  }

  async refreshSessions(): Promise<void> {
    const seq = ++this.loadSeq;
    try {
      const r = await this.client.request<SessionListResult>("agent/sessionList", this.wsId ? { workspaceId: this.wsId } : {});
      if (seq === this.loadSeq) this.set({ sessions: r.sessions });
    } catch (e) {
      if (seq === this.loadSeq) this.set({ error: describeAgentError(e) });
    }
  }

  setSearch(search: string): void {
    this.set({ search });
  }

  setScope(scope: Scope): void {
    if (scope === this.state.scope) return;
    this.set({ scope, sessions: [] });
    this.newChat(scope);
    void this.load();
  }

  setModel(model: ModelChoice): void {
    this.set({ model });
    try {
      this.storage?.setItem(MODEL_KEY, JSON.stringify(model));
    } catch {
      /* storage unavailable */
    }
  }

  /** A new, empty chat (created on the first send). */
  newChat(scope: Scope = this.state.scope): void {
    this.set({ scope, activeId: null, active: EMPTY, sendError: "" });
  }

  async open(id: string): Promise<void> {
    this.set({ activeId: id, active: this.cache.get(id) ?? EMPTY, sendError: "" });
    try {
      const r = await this.client.request<SessionOpenResult>("agent/sessionOpen", { workspaceId: this.wsId, sessionId: id });
      const prev = this.cache.get(id);
      // Events that arrived while the replay was on its way are kept (dedupe by seq).
      const extra = (prev?.events ?? []).filter((e) => e.seq > (r.events.at(-1)?.seq ?? 0));
      const conv: Conversation = { events: [...r.events, ...extra], streaming: prev?.streaming ?? "", running: r.session.running };
      this.cache.set(id, conv);
      const approvals = [...this.state.approvals.filter((a) => a.sessionId !== id), ...(r.pending ?? [])];
      if (this.state.activeId === id) this.set({ active: conv, approvals });
    } catch (e) {
      if (this.state.activeId === id) this.set({ sendError: describeAgentError(e) });
    }
  }

  /** Sends a message (creating the conversation first for a new chat). */
  async send(text: string, images: ImageData[] = [], fromTurn?: number): Promise<boolean> {
    if (!text.trim() && images.length === 0) return false;
    this.set({ sendError: "" });
    let id = this.state.activeId;
    try {
      if (!id) {
        const m = this.state.model;
        if (!m) {
          this.set({ sendError: "Choose a model first." });
          return false;
        }
        const s = await this.client.request<SessionInfo>("agent/sessionCreate", {
          workspaceId: this.wsId,
          providerId: m.providerId,
          model: m.model,
        });
        id = s.sessionId;
        this.cache.set(id, { ...EMPTY });
        this.set({ activeId: id, active: this.cache.get(id) ?? EMPTY, sessions: [s, ...this.state.sessions] });
      }
      this.patch(id, { running: true });
      await this.client.request("agent/send", {
        workspaceId: this.wsId,
        sessionId: id,
        text,
        images: images.length ? images : undefined,
        fromTurn,
      });
      return true;
    } catch (e) {
      if (id) this.patch(id, { running: false });
      this.set({ sendError: describeAgentError(e) });
      return false;
    }
  }

  stop(): void {
    const id = this.state.activeId;
    if (id) void this.client.request("agent/cancel", { workspaceId: this.wsId, sessionId: id }).catch(() => {});
  }

  /** Edit: resend `text` as turn `turn`. */
  edit(turn: number, text: string): Promise<boolean> {
    return this.send(text, [], turn);
  }

  /** Regenerate the last answer: the last user message again, from its turn. */
  regenerate(): Promise<boolean> {
    const u = lastUser(transcript(this.state.active.events));
    return u ? this.send(u.text, [], u.turn) : Promise.resolve(false);
  }

  async rename(id: string, title: string): Promise<void> {
    try {
      const s = await this.client.request<SessionInfo>("agent/sessionRename", { workspaceId: this.wsId, sessionId: id, title });
      this.set({ sessions: this.state.sessions.map((x) => (x.sessionId === id ? s : x)) });
    } catch (e) {
      this.set({ error: describeAgentError(e) });
    }
  }

  async remove(id: string): Promise<void> {
    try {
      await this.client.request("agent/sessionDelete", { workspaceId: this.wsId, sessionId: id });
      this.cache.delete(id);
      this.set({ sessions: this.state.sessions.filter((s) => s.sessionId !== id) });
      if (this.state.activeId === id) this.newChat();
    } catch (e) {
      this.set({ error: describeAgentError(e) });
    }
  }

  /** Answers an approval request (the first answer wins; a late one is ignored). */
  async approve(requestId: string, decision: "once" | "session" | "deny"): Promise<void> {
    this.set({ approvals: this.state.approvals.filter((a) => a.requestId !== requestId) });
    try {
      await this.client.request("agent/approve", { requestId, decision });
    } catch {
      /* already answered elsewhere, or withdrawn */
    }
  }

  /** Restores a checkpoint of the open conversation. Conflicts are held for the person's
   * choice (`restoreConflict`); `skipConflicts` restores the rest. Returns the result. */
  async restore(checkpoint: number, skipConflicts = false): Promise<RestoreResult | null> {
    const id = this.state.activeId;
    if (!id) return null;
    try {
      const r = await this.client.request<RestoreResult>("agent/restore", { workspaceId: this.wsId, sessionId: id, checkpoint, skipConflicts });
      this.set({ restoreConflict: null });
      return r;
    } catch (e) {
      if (e instanceof RpcError && e.kind === "checkpoint_conflict") {
        const conflicts = (Array.isArray(e.details.conflicts) ? e.details.conflicts : []) as RestoreConflict[];
        this.set({ restoreConflict: { checkpoint, conflicts } });
      } else {
        this.set({ error: describeAgentError(e), restoreConflict: null });
      }
      return null;
    }
  }

  dismissRestoreConflict(): void {
    this.set({ restoreConflict: null });
  }

  /** Approvals of conversations other than the open one, and of external principals (MCP). */
  otherApprovals(): ApprovalRequest[] {
    return this.state.approvals.filter((a) => !a.sessionId || a.sessionId !== this.state.activeId);
  }

  /** Fetches an image of the open conversation for display (once). */
  async loadImage(ref: ImageRef): Promise<void> {
    const id = this.state.activeId;
    if (!id || this.state.images[ref.sha256]) return;
    try {
      const d = await this.client.request<ImageData>("agent/image", { workspaceId: this.wsId, sessionId: id, sha256: ref.sha256 });
      this.set({ images: { ...this.state.images, [ref.sha256]: `data:${d.mediaType};base64,${d.data}` } });
    } catch {
      /* gone: the thumbnail shows its placeholder */
    }
  }

  dismissError(): void {
    this.set({ error: "", sendError: "" });
  }

  /** The open conversation's transcript items. */
  items(): Item[] {
    return transcript(this.state.active.events);
  }

  private patch(id: string, p: Partial<Conversation>): void {
    const conv = { ...(this.cache.get(id) ?? EMPTY), ...p };
    this.cache.set(id, conv);
    const sessions = p.running === undefined ? this.state.sessions : this.state.sessions.map((s) => (s.sessionId === id ? { ...s, running: p.running ?? s.running } : s));
    if (this.state.activeId === id) this.set({ active: conv, sessions });
    else this.set({ sessions });
  }

  private onEvent(e: AgentEvent): void {
    const id = e.sessionId;
    const conv = this.cache.get(id);
    if (!conv) return; // a session this window never opened
    if (e.event.type === "textDelta") {
      this.pendingDelta.set(id, (this.pendingDelta.get(id) ?? "") + e.event.text);
      if (!this.flushScheduled) {
        this.flushScheduled = true;
        frame(() => this.flushDeltas());
      }
      return;
    }
    const last = conv.events.at(-1)?.seq ?? 0;
    if (e.seq <= last) return; // already have it (replay overlap)
    this.flushDeltas();
    const current = this.cache.get(id) ?? conv;
    const events = [...current.events, e];
    let streaming = current.streaming;
    let running = current.running;
    if (e.event.type === "assistantText" || e.event.type === "toolCall") streaming = "";
    if (e.event.type === "user") running = true;
    if (e.event.type === "turnEnd") {
      streaming = "";
      running = false;
    }
    this.patch(id, { events, streaming, running });
    if (e.event.type === "turnEnd" || e.event.type === "user") {
      this.set({
        sessions: this.state.sessions.map((s) => (s.sessionId === id ? { ...s, updatedMs: e.tsMs, running } : s)),
      });
    }
    // The daemon titles a conversation from its first message: pick that up.
    if (e.event.type === "turnEnd" && e.turn === 1) void this.refreshSessions();
  }

  private flushDeltas(): void {
    this.flushScheduled = false;
    if (this.pendingDelta.size === 0) return;
    const pending = this.pendingDelta;
    this.pendingDelta = new Map();
    for (const [id, text] of pending) {
      const conv = this.cache.get(id);
      if (conv) this.patch(id, { streaming: conv.streaming + text });
    }
  }
}
