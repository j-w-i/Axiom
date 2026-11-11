// The UI's only route to axiomd. Equivalent of the Qt shell's DaemonClient
// seen from the models: request → Promise (answered exactly once), typed
// notifications, connection state. The transport (Tauri host, or a Node
// stdio process in tests) owns framing, lifecycle and timeouts.

import { RpcError, type BridgeEvent, type DaemonState, type HostEvent } from "./types";

export interface Transport {
  /** Starts delivering host events in order. Called once. */
  subscribe(onEvent: (e: HostEvent) => void): Promise<void>;
  request(key: number, method: string, params: unknown, timeoutMs?: number): Promise<void>;
  cancel(key: number): Promise<void>;
  restart(): Promise<void>;
}

type Listener<T> = (value: T) => void;

export class DaemonClient {
  private nextKey = 0;
  private readonly pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: RpcError) => void }>();
  private readonly notificationListeners = new Map<string, Set<Listener<unknown>>>();
  private readonly stateListeners = new Set<Listener<DaemonState>>();
  private readonly closeListeners = new Set<() => void>();
  private current: DaemonState = { state: "starting" };
  private generation = -1;

  constructor(private readonly transport: Transport) {}

  async connect(): Promise<void> {
    await this.transport.subscribe((e) => this.onHostEvent(e));
  }

  get state(): DaemonState {
    return this.current;
  }

  get ready(): boolean {
    return this.current.state === "ready";
  }

  /** Resolves with the result or rejects with an RpcError — exactly once. */
  request<T>(method: string, params: unknown = {}, timeoutMs?: number): Promise<T> {
    const key = ++this.nextKey;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(key, { resolve: resolve as (v: unknown) => void, reject });
      this.transport.request(key, method, params, timeoutMs).catch((e: unknown) => {
        if (this.pending.delete(key)) {
          reject(new RpcError({ code: -32603, message: String(e), data: { kind: "internal" } }));
        }
      });
    });
  }

  /** Like `request`, plus a way to send `$/cancel` for it. */
  cancellable<T>(method: string, params: unknown = {}): { promise: Promise<T>; cancel: () => void } {
    const key = this.nextKey + 1;
    const promise = this.request<T>(method, params);
    return { promise, cancel: () => void this.transport.cancel(key) };
  }

  restart(): Promise<void> {
    return this.transport.restart();
  }

  onNotification<T>(method: string, fn: Listener<T>): () => void {
    let set = this.notificationListeners.get(method);
    if (!set) this.notificationListeners.set(method, (set = new Set()));
    set.add(fn as Listener<unknown>);
    return () => set.delete(fn as Listener<unknown>);
  }

  onState(fn: Listener<DaemonState>): () => void {
    this.stateListeners.add(fn);
    return () => this.stateListeners.delete(fn);
  }

  onCloseRequested(fn: () => void): () => void {
    this.closeListeners.add(fn);
    return () => this.closeListeners.delete(fn);
  }

  private onHostEvent(e: HostEvent): void {
    if (e.kind === "closeRequested") {
      for (const fn of [...this.closeListeners]) fn();
      return;
    }
    this.onBridgeEvent(e.event);
  }

  private onBridgeEvent(e: BridgeEvent): void {
    switch (e.type) {
      case "response": {
        const p = this.pending.get(e.key);
        if (!p) return;
        this.pending.delete(e.key);
        if (e.error) p.reject(new RpcError(e.error));
        else p.resolve(e.result ?? null);
        return;
      }
      case "notification": {
        const set = this.notificationListeners.get(e.method);
        if (set) for (const fn of [...set]) fn(e.params);
        return;
      }
      case "state": {
        if (e.generation < this.generation) return;
        this.generation = e.generation;
        this.current = e.state;
        for (const fn of [...this.stateListeners]) fn(e.state);
        return;
      }
      case "log":
        return;
    }
  }
}
