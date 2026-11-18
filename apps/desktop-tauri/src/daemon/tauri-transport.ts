import { Channel, invoke } from "@tauri-apps/api/core";
import type { Transport } from "./client";
import type { HostEvent } from "./types";

/** Talks to the Rust host (src-tauri/src/lib.rs) through its seven commands. */
export class TauriTransport implements Transport {
  async subscribe(onEvent: (e: HostEvent) => void): Promise<void> {
    const channel = new Channel<HostEvent>();
    channel.onmessage = onEvent;
    await invoke("host_subscribe", { channel });
  }

  async request(key: number, method: string, params: unknown, timeoutMs?: number): Promise<void> {
    await invoke("daemon_request", { key, method, params, timeoutMs: timeoutMs ?? null });
  }

  async cancel(key: number): Promise<void> {
    await invoke("daemon_cancel", { key });
  }

  async restart(): Promise<void> {
    await invoke("daemon_restart");
  }
}

export interface RecoverySnapshot {
  recovered: boolean;
  snapshots: Record<string, string>;
}

export interface AppInfo {
  appVersion: string;
  tauriVersion: string;
  webviewVersion: string;
  protocolVersion: string;
  dataDir: string | null;
  os: string;
  uptimeMs: number;
  pid: number;
}

export const host = {
  appInfo: () => invoke<AppInfo>("app_info"),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  quit: () => invoke<void>("app_quit"),
  /** Proves to the host that the page is alive (crash/hang detection, T-08). */
  heartbeat: () => invoke<void>("host_heartbeat"),
  /** Keeps the latest unsaved text of a document in the host (memory only). */
  snapshotPut: (path: string, text: string) => invoke<void>("snapshot_put", { path, text }),
  snapshotClear: (path: string) => invoke<void>("snapshot_clear", { path }),
  /** What a freshly loaded page must restore after a crash. */
  recoveryState: () => invoke<RecoverySnapshot>("recovery_state"),
};
