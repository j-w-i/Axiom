// Port of SettingsStore: the daemon is the single source of truth. A value
// changes when `settings/didChange` (or the initial load) arrives, never
// optimistically; a rejected value reports an error and leaves it unchanged.

import type { DaemonClient } from "../daemon/client";
import { RpcError } from "../daemon/types";
import { Observable } from "./observable";

export type ThemeSetting = "system" | "light" | "dark";
/** `appearance.motion`: a shell-side key the daemon stores unchanged (no protocol change). */
export type MotionSetting = "standard" | "reduced" | "off";

export interface SettingsState {
  loaded: boolean;
  theme: ThemeSetting;
  motion: MotionSetting;
  editorFontSize: number;
  editorFontFamily: string;
  editorWordWrap: boolean;
  reopenLast: boolean;
  error: string;
}

const KEYS: Record<string, keyof SettingsState> = {
  "appearance.theme": "theme",
  "appearance.motion": "motion",
  "editor.fontSize": "editorFontSize",
  "editor.fontFamily": "editorFontFamily",
  "editor.wordWrap": "editorWordWrap",
  "workspace.reopenLast": "reopenLast",
};

export class SettingsStore extends Observable<SettingsState> {
  constructor(private readonly client: DaemonClient) {
    // Defaults mirror the table in ipc.md; real values arrive from the daemon.
    super({ loaded: false, theme: "system", motion: "standard", editorFontSize: 13, editorFontFamily: "", editorWordWrap: false, reopenLast: true, error: "" });
    client.onNotification<{ values: Record<string, unknown> }>("settings/didChange", (p) => this.apply(p.values));
    client.onState((s) => {
      if (s.state === "ready") void this.load();
    });
  }

  async load(): Promise<void> {
    try {
      const r = await this.client.request<{ values: Record<string, unknown> }>("settings/get", {});
      this.apply(r.values);
      if (!this.state.loaded) this.set({ loaded: true });
    } catch (e) {
      this.set({ error: `Could not read the settings: ${(e as RpcError).message}` });
    }
  }

  async setValue(key: keyof typeof KEYS | string, value: unknown): Promise<void> {
    try {
      await this.client.request("settings/set", { values: { [key]: value } });
    } catch (e) {
      const err = e as RpcError;
      this.set({
        error: err.kind === "invalid_params"
          ? `That value is not valid for ${String(err.details.key ?? key)}.`
          : `Could not save the setting: ${err.message}`,
      });
    }
  }

  dismissError(): void {
    this.set({ error: "" });
  }

  private apply(values: Record<string, unknown>): void {
    const patch: Partial<SettingsState> = {};
    for (const [wire, field] of Object.entries(KEYS)) {
      if (!(wire in values)) continue;
      let v = values[wire];
      // Any value other than the three levels (or none) reads as "standard".
      if (wire === "appearance.motion" && v !== "reduced" && v !== "off") v = "standard";
      const current = this.state[field];
      if (typeof v === typeof current && v !== current) (patch as Record<string, unknown>)[field] = v;
    }
    if (Object.keys(patch).length > 0) this.set(patch);
  }
}
