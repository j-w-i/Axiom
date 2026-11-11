// Test-only Transport: spawns the real axiomd and speaks protocol 0.1 over
// its stdio from Node, with the same event semantics as the Rust bridge
// (responses delivered as ordered `response` events). Lets the UI stores be
// tested against the real daemon without a webview, like the Qt tst_* suites.

import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Transport } from "../../src/daemon/client";
import type { HostEvent } from "../../src/daemon/types";

export function daemonPath(): string {
  return process.env.AXIOMD_UNDER_TEST ?? join(import.meta.dirname, "../../../../target/debug/axiomd");
}

export class NodeTransport implements Transport {
  private proc: ChildProcessWithoutNullStreams | null = null;
  private onEvent: (e: HostEvent) => void = () => {};
  private buf = "";
  private nextId = 0;
  private generation = 0;
  private readonly keys = new Map<number, number>(); // wire id -> key
  readonly dataDir = mkdtempSync(join(tmpdir(), "axiom-tauri-test-"));

  /** Like the real host: subscribing again (a reloaded page) replays the
   * current state instead of starting another daemon. */
  async subscribe(onEvent: (e: HostEvent) => void): Promise<void> {
    this.onEvent = onEvent;
    if (this.proc) {
      this.emitState(this.currentState ?? { state: "starting" });
      return;
    }
    await this.start();
  }

  private currentState: unknown = null;

  private emitState(state: unknown): void {
    this.currentState = state;
    this.onEvent({ kind: "bridge", event: { type: "state", generation: this.generation, state } as never });
  }

  private async start(): Promise<void> {
    this.generation++;
    this.emitState({ state: "starting" });
    const proc = spawn(daemonPath(), ["--data-dir", this.dataDir], { stdio: "pipe" });
    this.proc = proc;
    this.buf = "";
    proc.stdout.setEncoding("utf8");
    proc.stdout.on("data", (chunk: string) => {
      if (proc !== this.proc) return;
      this.buf += chunk;
      let nl: number;
      while ((nl = this.buf.indexOf("\n")) >= 0) {
        const line = this.buf.slice(0, nl).trim();
        this.buf = this.buf.slice(nl + 1);
        if (line) this.onLine(line);
      }
    });
    proc.on("exit", () => {
      if (proc !== this.proc) return;
      this.proc = null;
      for (const key of this.keys.values()) {
        this.onEvent({ kind: "bridge", event: { type: "response", key, error: { code: -32603, message: "Backend stopped", data: { kind: "internal" } } } });
      }
      this.keys.clear();
      this.emitState({ state: "failed", message: "The backend exited unexpectedly." });
    });
    const init = await this.rawRequest("initialize", { protocolVersion: "0.1", client: { name: "test", version: "0" } });
    this.emitState({ state: "ready", serverVersion: (init as { server: { version: string } }).server.version, protocolVersion: "0.1", methods: [] });
  }

  private initWaiters = new Map<number, (v: unknown) => void>();

  private rawRequest(method: string, params: unknown): Promise<unknown> {
    const id = ++this.nextId;
    return new Promise((resolve) => {
      this.initWaiters.set(id, resolve);
      this.proc?.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }

  private onLine(line: string): void {
    const msg = JSON.parse(line) as { id?: number; method?: string; params?: unknown; result?: unknown; error?: never };
    if (msg.method !== undefined && msg.id === undefined) {
      this.onEvent({ kind: "bridge", event: { type: "notification", method: msg.method, params: msg.params ?? null } });
      return;
    }
    if (typeof msg.id !== "number") return;
    const w = this.initWaiters.get(msg.id);
    if (w) {
      this.initWaiters.delete(msg.id);
      w(msg.result);
      return;
    }
    const key = this.keys.get(msg.id);
    if (key === undefined) return;
    this.keys.delete(msg.id);
    this.onEvent({ kind: "bridge", event: { type: "response", key, result: msg.result, error: msg.error } });
  }

  async request(key: number, method: string, params: unknown): Promise<void> {
    if (!this.proc) {
      this.onEvent({ kind: "bridge", event: { type: "response", key, error: { code: -32603, message: "Backend stopped", data: { kind: "internal" } } } });
      return;
    }
    const id = ++this.nextId;
    this.keys.set(id, key);
    this.proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  }

  async cancel(key: number): Promise<void> {
    for (const [id, k] of this.keys) {
      if (k === key) this.proc?.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "$/cancel", params: { id } }) + "\n");
    }
  }

  async restart(): Promise<void> {
    await this.stop();
    await this.start();
  }

  kill(): void {
    this.proc?.kill("SIGKILL");
  }

  async stop(): Promise<void> {
    const p = this.proc;
    if (!p) return;
    this.proc = null;
    await new Promise<void>((resolve) => {
      p.on("exit", () => resolve());
      p.stdin.end();
      setTimeout(() => p.kill("SIGKILL"), 2000);
    });
  }

  async dispose(): Promise<void> {
    await this.stop();
    rmSync(this.dataDir, { recursive: true, force: true });
  }
}

export async function waitFor(cond: () => boolean, ms = 5000, what = "condition"): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (cond()) return;
    await new Promise((r) => setTimeout(r, 10));
  }
  throw new Error(`timed out waiting for ${what}`);
}
