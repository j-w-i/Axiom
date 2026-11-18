// The Environment Manager's state (M2-UI-001, M2-UI-002): which providers apply to the open
// workspace, what each tool's state is, install plans and their progress. Everything comes
// from `env/*` (ADR-0014); the UI never edits a plan, it shows what the daemon returned.

import type { DaemonClient } from "../daemon/client";
import type {
  DetectResult,
  InstallPlan,
  InstallProgress,
  InstallResult,
  ProviderDetection,
  ProviderInfo,
  ProvidersResult,
  ToolStatus,
} from "../daemon/protocol.gen";
import { RpcError } from "../daemon/types";
import { Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export type RowState =
  | { kind: "ready" }
  | { kind: "missing" }
  | { kind: "installing"; progress: InstallProgress | null }
  | { kind: "error"; message: string };

/** One provider that applies to the workspace, as a table row. */
export interface ToolchainRow {
  providerId: string;
  name: string;
  /** Tool ids, required ones first. */
  tools: string[];
  /** What `.axiom/project.toml` selects, if anything. */
  selected: string | null;
  /** The version of the first required tool found. */
  detected: string | null;
  source: { kind: "managed" | "system"; where: string } | null;
  /** A managed version that can be removed (the selected one, else the detected one). */
  removable: string | null;
  state: RowState;
}

export interface EnvironmentsState {
  status: "idle" | "loading" | "ready" | "error";
  error: string;
  providers: ProviderInfo[];
  detection: ProviderDetection[];
  /** providerId → the plan being installed and its latest progress. */
  installs: Record<string, { planId: string; progress: InstallProgress | null }>;
  /** providerId → the last install failure, in plain language. */
  failures: Record<string, string>;
  /** The plan shown for approval (UI-002), and its provider's display name. */
  plan: { plan: InstallPlan; providerName: string } | null;
  planning: string | null;
  planError: string;
}

const initial: EnvironmentsState = {
  status: "idle",
  error: "",
  providers: [],
  detection: [],
  installs: {},
  failures: {},
  plan: null,
  planning: null,
  planError: "",
};

/** An env/* failure as one sentence for the page. */
export function describeError(e: unknown): string {
  if (!(e instanceof RpcError)) return String(e);
  switch (e.kind) {
    case "checksum_mismatch":
      return "The download did not match its published checksum, so nothing was installed.";
    case "approval_denied":
      return "The plan expired or changed before it ran. Review a new plan.";
    case "unsupported_platform":
      return "There is no install for this platform.";
    case "prerequisite_missing": {
      const missing = Array.isArray(e.details.missing) ? (e.details.missing as string[]).join(", ") : "";
      return missing ? `Needs ${missing} first.` : e.message;
    }
    case "cancelled":
      return "The install was cancelled.";
    default:
      return e.message;
  }
}

function sourceOf(t: ToolStatus | undefined): ToolchainRow["source"] {
  const loc = t?.found?.location;
  if (!loc) return null;
  return loc.kind === "managed" ? { kind: "managed", where: loc.root } : { kind: "system", where: loc.path };
}

/** The table, from the daemon's answers. Pure. */
export function toolchainRows(s: EnvironmentsState): ToolchainRow[] {
  return s.detection.map((d) => {
    const info = s.providers.find((p) => p.id === d.providerId && p.trusted && !p.errors?.length);
    const required = d.tools.filter((t) => t.required);
    const tools = [...required, ...d.tools.filter((t) => !t.required)].map((t) => t.toolId);
    const firstFound = required.find((t) => t.found) ?? d.tools.find((t) => t.found);
    const allRequired = required.every((t) => t.found);
    const source = sourceOf(firstFound);
    const install = s.installs[d.providerId];
    const failure = s.failures[d.providerId];
    const state: RowState = install
      ? { kind: "installing", progress: install.progress }
      : failure
        ? { kind: "error", message: failure }
        : allRequired
          ? { kind: "ready" }
          : { kind: "missing" };
    const managedVersion = source?.kind === "managed" ? (source.where.split(/[\\/]/).pop() ?? null) : null;
    return {
      providerId: d.providerId,
      name: info?.name ?? d.providerId,
      tools,
      selected: d.selectedVersion ?? null,
      detected: firstFound?.found?.version ?? null,
      source,
      removable: managedVersion,
      state,
    };
  });
}

/** Workspace providers the user has not trusted yet (listed, never loaded). */
export const untrusted = (s: EnvironmentsState): ProviderInfo[] => s.providers.filter((p) => p.source === "workspace" && !p.trusted);

/** Manifests that failed validation. */
export const invalid = (s: EnvironmentsState): ProviderInfo[] => s.providers.filter((p) => (p.errors?.length ?? 0) > 0);

export class EnvironmentsStore extends Observable<EnvironmentsState> {
  /** planId → providerId, for plans this window started. */
  private readonly planProvider = new Map<string, string>();
  private readonly cancels = new Map<string, () => void>();
  private generation = 0;

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super(initial);
    // Detection runs the providers' version commands (tens of ms); its result also feeds the
    // rail's badge, so it runs whenever a workspace opens, after the first paint's work.
    workspace.on("opened", () => void this.refresh());
    workspace.on("closed", () => {
      this.generation++;
      this.set({ ...initial });
    });
    client.onNotification<InstallProgress>("env/installProgress", (p) => {
      const providerId = this.planProvider.get(p.planId);
      if (!providerId || !this.state.installs[providerId]) return;
      this.set({ installs: { ...this.state.installs, [providerId]: { planId: p.planId, progress: p } } });
    });
    client.onNotification<InstallResult>("env/installed", () => void this.refresh());
  }

  /** Providers and detection for the open workspace. */
  async refresh(): Promise<void> {
    const workspaceId = this.workspace.getSnapshot().id;
    if (!workspaceId) return;
    const gen = ++this.generation;
    this.set({ status: this.state.status === "ready" ? "ready" : "loading", error: "" });
    try {
      const [providers, detection] = await Promise.all([
        this.client.request<ProvidersResult>("env/providers", { workspaceId }),
        this.client.request<DetectResult>("env/detect", { workspaceId }),
      ]);
      if (gen !== this.generation) return;
      this.set({ status: "ready", providers: providers.providers, detection: detection.providers });
    } catch (e) {
      if (gen !== this.generation) return;
      this.set({ status: "error", error: describeError(e) });
    }
  }

  async trustWorkspace(trusted: boolean): Promise<void> {
    const workspaceId = this.workspace.getSnapshot().id;
    if (!workspaceId) return;
    try {
      await this.client.request("env/trust", { workspaceId, trusted });
    } catch (e) {
      this.set({ error: describeError(e) });
    }
    await this.refresh();
  }

  async uninstall(providerId: string, version: string): Promise<void> {
    try {
      await this.client.request("env/uninstall", { providerId, version });
      const failures = { ...this.state.failures };
      delete failures[providerId];
      this.set({ failures });
    } catch (e) {
      this.set({ failures: { ...this.state.failures, [providerId]: describeError(e) } });
    }
    await this.refresh();
  }

  /** Asks the daemon for a plan: the selected version, else the newest available. */
  async planInstall(providerId: string): Promise<void> {
    const row = toolchainRows(this.state).find((r) => r.providerId === providerId);
    this.set({ planning: providerId, planError: "" });
    try {
      let version = row?.selected ?? null;
      if (!version || version === "system") {
        const v = await this.client.request<{ available: string[] }>("env/versions", { providerId });
        version = v.available[0] ?? null;
      }
      if (!version) throw new Error("The provider lists no installable versions.");
      const plan = await this.client.request<InstallPlan>("env/planInstall", { providerId, version });
      this.set({ plan: { plan, providerName: row?.name ?? providerId }, planning: null });
    } catch (e) {
      this.set({ planning: null, planError: describeError(e), failures: { ...this.state.failures, [providerId]: describeError(e) } });
    }
  }

  /** Closes the plan dialog without calling the daemon. */
  dismissPlan(): void {
    this.set({ plan: null });
  }

  /** Runs the plan shown in the dialog (the approval). Resolves when the install ends. */
  async approvePlan(): Promise<void> {
    const shown = this.state.plan;
    if (!shown) return;
    const { planId, providerId } = shown.plan;
    this.planProvider.set(planId, providerId);
    const failures = { ...this.state.failures };
    delete failures[providerId];
    this.set({ plan: null, failures, installs: { ...this.state.installs, [providerId]: { planId, progress: null } } });
    const { promise, cancel } = this.client.cancellable<InstallResult>("env/install", { planId });
    this.cancels.set(providerId, cancel);
    try {
      await promise;
    } catch (e) {
      this.set({ failures: { ...this.state.failures, [providerId]: describeError(e) } });
    } finally {
      this.cancels.delete(providerId);
      this.planProvider.delete(planId);
      const installs = { ...this.state.installs };
      delete installs[providerId];
      this.set({ installs });
      await this.refresh();
    }
  }

  /** `$/cancel` for a running install. */
  cancelInstall(providerId: string): void {
    this.cancels.get(providerId)?.();
  }

  /** The install that is running, if any (the transport shows it). */
  get activeInstall(): { providerId: string; name: string; progress: InstallProgress | null } | null {
    const [providerId, i] = Object.entries(this.state.installs)[0] ?? [];
    if (!providerId || !i) return null;
    const name = this.state.providers.find((p) => p.id === providerId)?.name ?? providerId;
    return { providerId, name, progress: i.progress };
  }
}
