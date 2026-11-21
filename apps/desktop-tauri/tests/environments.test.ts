// The Environment Manager's store and pure helpers (M2-UI-001, M2-UI-002).
import { describe, expect, it } from "vitest";
import { effectNotes, formatBytes, progressValue } from "../src/components/Environments";
import type { DaemonClient } from "../src/daemon/client";
import type { DetectResult, InstallPlan, ProvidersResult } from "../src/daemon/protocol.gen";
import { RpcError } from "../src/daemon/types";
import { railTarget } from "../src/shell/Rail";
import { describeError, EnvironmentsStore, invalid, toolchainRows, untrusted, type EnvironmentsState } from "../src/state/environments";
import type { WorkspaceStore } from "../src/state/workspace";

const providers: ProvidersResult = {
  providers: [
    { id: "rust", name: "Rust", description: "", version: "0.2.0", source: "builtin", trusted: true },
    { id: "local", name: "Local", description: "", version: "0.1.0", source: "workspace", trusted: false },
    { id: "broken", name: "broken", description: "", version: "", source: "user", trusted: true, errors: ["line 3: unknown field `x`"] },
  ],
};

const found = (tool: string, version: string, location: { kind: "managed"; root: string } | { kind: "system"; path: string }) => ({
  toolId: tool,
  required: true,
  found: { providerId: "rust", toolId: tool, version, location, verifiedMs: 1 },
});

const detected: DetectResult = {
  providers: [
    {
      providerId: "rust",
      selectedVersion: "1.84.0",
      tools: [
        found("cargo", "1.84.0", { kind: "managed", root: "/data/toolchains/rust/1.84.0" }),
        found("rustc", "1.84.0", { kind: "managed", root: "/data/toolchains/rust/1.84.0" }),
        { toolId: "rustup", required: false },
      ],
    },
  ],
};

const base: EnvironmentsState = {
  status: "ready",
  error: "",
  providers: providers.providers,
  detection: detected.providers,
  installs: {},
  failures: {},
  plan: null,
  planning: null,
  planError: "",
};

describe("environment rows", () => {
  it("ready managed toolchain with its selection, tools and removable version", () => {
    const r = toolchainRows(base)[0]!;
    expect(r).toMatchObject({
      providerId: "rust",
      name: "Rust",
      tools: ["cargo", "rustc", "rustup"],
      selected: "1.84.0",
      detected: "1.84.0",
      source: { kind: "managed", where: "/data/toolchains/rust/1.84.0" },
      removable: "1.84.0",
      state: { kind: "ready" },
    });
  });

  it("missing when a required tool is not found; system tools are not removable", () => {
    const s: EnvironmentsState = {
      ...base,
      detection: [{ providerId: "rust", tools: [found("cargo", "1.90.0", { kind: "system", path: "/usr/bin/cargo" }), { toolId: "rustc", required: true }] }],
    };
    const r = toolchainRows(s)[0]!;
    expect(r.state).toEqual({ kind: "missing" });
    expect(r.source).toEqual({ kind: "system", where: "/usr/bin/cargo" });
    expect(r.removable).toBeNull();
    expect(r.selected).toBeNull();
  });

  it("installing wins over failure and readiness; failure shows its message", () => {
    const progress = { planId: "p", step: 0, steps: 3, label: "Downloading x" };
    expect(toolchainRows({ ...base, installs: { rust: { planId: "p", progress } } })[0]!.state).toEqual({ kind: "installing", progress });
    expect(toolchainRows({ ...base, failures: { rust: "bad" } })[0]!.state).toEqual({ kind: "error", message: "bad" });
  });

  it("untrusted and invalid providers are listed apart", () => {
    expect(untrusted(base).map((p) => p.id)).toEqual(["local"]);
    expect(invalid(base).map((p) => p.id)).toEqual(["broken"]);
  });
});

describe("helpers", () => {
  it("error kinds in plain language", () => {
    const err = (kind: string, details = {}) => new RpcError({ code: -1, message: "raw", data: { kind: kind as never, details } });
    expect(describeError(err("checksum_mismatch"))).toMatch(/did not match its published checksum/);
    expect(describeError(err("approval_denied"))).toMatch(/Review a new plan/);
    expect(describeError(err("prerequisite_missing", { missing: ["pkexec"] }))).toBe("Needs pkexec first.");
    expect(describeError(err("io"))).toBe("raw");
  });

  it("progress: steps plus the fraction of the current download", () => {
    expect(progressValue(null)).toBe(0);
    expect(progressValue({ planId: "p", step: 0, steps: 4, label: "", bytesDone: 50, bytesTotal: 100 })).toBe(0.125);
    expect(progressValue({ planId: "p", step: 3, steps: 4, label: "" })).toBe(0.75);
  });

  it("sizes and effect notes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(21_347_088)).toBe("20.4 MB");
    const plan = { scope: "system", effects: ["network", "systemChange", "elevation"] } as unknown as InstallPlan;
    expect(effectNotes(plan)).toEqual([
      "This changes your system outside Axiom's folders.",
      "Your system will ask for an administrator password; Axiom never sees it.",
    ]);
  });

  it("rail keys wrap and jump", () => {
    expect(railTarget("ArrowDown", 1, 2)).toBe(0);
    expect(railTarget("ArrowUp", 0, 2)).toBe(1);
    expect(railTarget("End", 0, 2)).toBe(1);
    expect(railTarget("Home", 1, 2)).toBe(0);
    expect(railTarget("a", 0, 2)).toBeNull();
  });
});

/** A client that answers env/* from a script and records calls. */
function fakeClient(answers: Record<string, (params: Record<string, unknown>) => unknown>) {
  const calls: { method: string; params: Record<string, unknown> }[] = [];
  const listeners = new Map<string, ((v: unknown) => void)[]>();
  let cancelled = false;
  let finishInstall: (() => void) | null = null;
  let failInstall: ((e: unknown) => void) | null = null;
  const client = {
    request: async (method: string, params: Record<string, unknown> = {}) => {
      calls.push({ method, params });
      const a = answers[method];
      if (!a) throw new Error(`unexpected ${method}`);
      return a(params);
    },
    cancellable: (method: string, params: Record<string, unknown>) => {
      calls.push({ method, params });
      const promise = new Promise((resolve, reject) => {
        finishInstall = () => resolve({});
        failInstall = reject;
      });
      return { promise, cancel: () => (cancelled = true) };
    },
    onNotification: (method: string, fn: (v: unknown) => void) => {
      listeners.set(method, [...(listeners.get(method) ?? []), fn]);
      return () => {};
    },
  } as unknown as DaemonClient;
  const emit = (method: string, v: unknown) => listeners.get(method)?.forEach((f) => f(v));
  return { client, calls, emit, isCancelled: () => cancelled, finish: () => finishInstall?.(), fail: (e: unknown) => failInstall?.(e) };
}

function fakeWorkspace(id = "w1") {
  return { getSnapshot: () => ({ id }), on: () => () => {} } as unknown as WorkspaceStore;
}

const plan: InstallPlan = {
  planId: "p1",
  providerId: "rust",
  version: "1.84.0",
  scope: "managed",
  steps: [{ kind: "verify", toolId: "cargo" }],
  effects: ["writeManaged"],
  expiresMs: 1,
};

describe("environments store", () => {
  it("refresh loads providers and detection for the open workspace", async () => {
    const f = fakeClient({ "env/providers": () => providers, "env/detect": () => detected });
    const store = new EnvironmentsStore(f.client, fakeWorkspace());
    await store.refresh();
    expect(store.getSnapshot().status).toBe("ready");
    expect(f.calls.map((c) => [c.method, c.params.workspaceId])).toEqual([
      ["env/providers", "w1"],
      ["env/detect", "w1"],
    ]);
  });

  it("plan uses the selected version, approval installs exactly that planId, progress maps to the row", async () => {
    const f = fakeClient({
      "env/providers": () => providers,
      "env/detect": () => ({ providers: [{ ...detected.providers[0]!, tools: [{ toolId: "cargo", required: true }] }] }),
      "env/planInstall": () => plan,
    });
    const store = new EnvironmentsStore(f.client, fakeWorkspace());
    await store.refresh();
    await store.planInstall("rust");
    expect(f.calls.find((c) => c.method === "env/planInstall")?.params).toEqual({ providerId: "rust", version: "1.84.0" });
    expect(store.getSnapshot().plan?.plan).toBe(plan);

    // Cancel in the dialog: nothing is sent.
    store.dismissPlan();
    expect(f.calls.some((c) => c.method === "env/install")).toBe(false);

    await store.planInstall("rust");
    const done = store.approvePlan();
    expect(f.calls.find((c) => c.method === "env/install")?.params).toEqual({ planId: "p1" });
    expect(store.getSnapshot().plan).toBeNull();
    f.emit("env/installProgress", { planId: "p1", step: 0, steps: 1, label: "Verifying cargo" });
    expect(toolchainRows(store.getSnapshot())[0]!.state).toMatchObject({ kind: "installing", progress: { label: "Verifying cargo" } });
    expect(store.activeInstall?.name).toBe("Rust");
    // Progress for a plan this window did not start is ignored.
    f.emit("env/installProgress", { planId: "other", step: 0, steps: 1, label: "x" });
    store.cancelInstall("rust");
    expect(f.isCancelled()).toBe(true);
    f.fail(new RpcError({ code: -32800, message: "Cancelled.", data: { kind: "cancelled" } }));
    await done;
    expect(store.activeInstall).toBeNull();
    expect(toolchainRows(store.getSnapshot())[0]!.state).toEqual({ kind: "error", message: "The install was cancelled." });
  });

  it("without a selection the newest available version is planned", async () => {
    const f = fakeClient({
      "env/providers": () => providers,
      "env/detect": () => ({ providers: [{ providerId: "rust", tools: [{ toolId: "cargo", required: true }] }] }),
      "env/versions": () => ({ available: ["1.90.0", "1.89.0"], installed: [] }),
      "env/planInstall": (p) => ({ ...plan, version: p.version }),
    });
    const store = new EnvironmentsStore(f.client, fakeWorkspace());
    await store.refresh();
    await store.planInstall("rust");
    expect(store.getSnapshot().plan?.plan.version).toBe("1.90.0");
  });
});
