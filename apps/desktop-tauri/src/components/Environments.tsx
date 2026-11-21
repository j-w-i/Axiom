// The Environment Manager (workspace-spec.md §5.1; M2-UI-001, M2-UI-002): an editor-area page
// listing the toolchains the open workspace needs, their detected versions and sources, and
// what can be done (Plan install…, Remove); the install-plan dialog that shows exactly the
// plan the daemon returned; and the side bar's summary of the same rows.

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { InstallPlan, InstallProgress, Step } from "../daemon/protocol.gen";
import {
  invalid,
  toolchainRows,
  untrusted,
  type EnvironmentsStore,
  type ToolchainRow,
} from "../state/environments";
import * as Icon from "../ui/icons";
import { Dialog } from "../ui/overlays";
import { Button, IconButton, Led, Meter, type LedState } from "../ui/primitives";
import { shortenHome } from "../ui/paths";
import "../styles/components/environments.css";

const PROJECT_FILE = ".axiom/project.toml";

/** 1.2 MB style sizes for plans. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** Fraction done for the meter: bytes within the download step, else steps done. */
export function progressValue(p: InstallProgress | null): number {
  if (!p || p.steps === 0) return 0;
  const within = p.bytesTotal ? (p.bytesDone ?? 0) / p.bytesTotal : 0;
  return Math.min(1, (p.step + within) / p.steps);
}

function ledOf(row: ToolchainRow): { state: LedState; text: string } {
  switch (row.state.kind) {
    case "ready":
      return { state: "on", text: "Ready" };
    case "installing":
      return { state: "run", text: "Installing" };
    case "error":
      return { state: "err", text: "Install failed" };
    default:
      return { state: "warn", text: "Missing" };
  }
}

function RowState({ row, store, planning }: { row: ToolchainRow; store: EnvironmentsStore; planning: boolean }) {
  const s = row.state;
  if (s.kind === "ready") {
    return (
      <span className="env-state env-state--ready">
        <Icon.Check />
        Ready
      </span>
    );
  }
  if (s.kind === "installing") {
    const label = s.progress?.label ?? "Starting";
    return (
      <span className="env-state env-state--progress">
        <Meter value={progressValue(s.progress)} label={`Installing ${row.name}`} />
        <span className="env-state__step" title={label}>
          {label}
        </span>
        <Button size="sm" variant="quiet" data-hint={`Stop installing ${row.name}`} onClick={() => store.cancelInstall(row.providerId)}>
          Cancel
        </Button>
      </span>
    );
  }
  return (
    <span className={s.kind === "error" ? "env-state env-state--error" : "env-state"}>
      {s.kind === "error" && (
        <span className="env-state__error" role="alert" title={s.message}>
          {s.message}
        </span>
      )}
      <Button
        variant="primary"
        size="sm"
        icon={<Icon.Download />}
        loading={planning}
        data-hint={`Review what installing ${row.name} would do`}
        onClick={() => void store.planInstall(row.providerId)}
      >
        Plan install…
      </Button>
    </span>
  );
}

function sourceText(row: ToolchainRow): string {
  if (!row.source) return "—";
  return `${row.source.kind} (${shortenHome(row.source.where)})`;
}

/** The page. `onOpenFolder` is used by the no-workspace state. */
export function EnvironmentPage({
  store,
  hasWorkspace,
  onOpenFolder,
}: {
  store: EnvironmentsStore;
  hasWorkspace: boolean;
  onOpenFolder: () => void;
}) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const [removing, setRemoving] = useState<ToolchainRow | null>(null);
  // The confirmation keeps naming its row while it animates out.
  const lastRemoving = useRef<ToolchainRow | null>(null);
  if (removing) lastRemoving.current = removing;
  const confirm = lastRemoving.current;
  useEffect(() => {
    if (hasWorkspace && s.status === "idle") void store.refresh();
  }, [store, hasWorkspace, s.status]);
  const rows = toolchainRows(s);
  const pending = untrusted(s);
  const broken = invalid(s);

  if (!hasWorkspace) {
    return (
      <section className="env-page" aria-label="Environments" data-testid="environments">
        <header className="env-page__head">
          <h1 className="env-page__title">Environments</h1>
        </header>
        <div className="env-empty">
          <p className="env-empty__title">Open a folder to see the toolchains it needs</p>
          <Button variant="primary" data-hint="Open a folder" onClick={onOpenFolder}>
            Open Folder…
          </Button>
        </div>
      </section>
    );
  }

  return (
    <section className="env-page" aria-label="Environments" data-testid="environments" data-status={s.status}>
      <header className="env-page__head">
        <h1 className="env-page__title">Environments</h1>
        <Button
          size="sm"
          icon={<Icon.Refresh />}
          loading={s.status === "loading"}
          data-hint="Detect the toolchains again"
          onClick={() => void store.refresh()}
        >
          Detect again
        </Button>
      </header>

      {s.status === "error" && (
        <div className="env-empty" role="alert">
          <p className="env-empty__title">The toolchains could not be detected</p>
          <p className="env-empty__body">{s.error}</p>
          <Button data-hint="Detect the toolchains again" onClick={() => void store.refresh()}>
            Retry
          </Button>
        </div>
      )}

      {s.status === "loading" && rows.length === 0 && (
        <p className="env-note" aria-live="polite">
          Detecting toolchains…
        </p>
      )}

      {s.status === "ready" && rows.length === 0 && (
        <div className="env-empty">
          <p className="env-empty__title">No toolchain provider applies to this folder</p>
          <p className="env-empty__body">
            Providers recognise projects by their files (for example <code>Cargo.toml</code>). Available:{" "}
            {s.providers
              .filter((p) => p.trusted && !p.errors?.length)
              .map((p) => p.name)
              .join(", ") || "none"}
            .
          </p>
        </div>
      )}

      {rows.length > 0 && (
        <table className="env-table" aria-label="Toolchains">
          <colgroup>
            <col className="env-col-name" />
            <col className="env-col-required" />
            <col className="env-col-detected" />
            <col />
            <col className="env-col-state" />
          </colgroup>
          <thead>
            <tr>
              <th scope="col">Toolchain</th>
              <th scope="col">Required by project</th>
              <th scope="col">Detected</th>
              <th scope="col">Source</th>
              <th scope="col" aria-label="State" />
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => {
              const led = ledOf(row);
              return (
                <tr key={row.providerId} data-provider={row.providerId} data-state={row.state.kind}>
                  <th scope="row">
                    <span className="env-name">
                      <Led state={led.state} label={`${row.name}: ${led.text}`} />
                      <span className="env-name__text">{row.name}</span>
                      <span className="env-name__tools">{row.tools.join(" · ")}</span>
                    </span>
                  </th>
                  <td className="env-mono">{row.selected ? `${row.selected} (${PROJECT_FILE})` : "—"}</td>
                  <td className="env-mono">{row.detected ?? "not found"}</td>
                  <td className="env-mono env-faint" title={row.source?.where}>
                    {sourceText(row)}
                  </td>
                  <td className="env-actions">
                    <RowState row={row} store={store} planning={s.planning === row.providerId} />
                    {row.removable && row.state.kind !== "installing" && (
                      <IconButton label={`Remove ${row.name} ${row.removable}`} onClick={() => setRemoving(row)}>
                        <Icon.Trash />
                      </IconButton>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}

      {pending.length > 0 && (
        <section className="env-section" aria-label="This folder's own providers">
          <h2 className="env-section__title">
            <Icon.Shield />
            This folder has its own providers
          </h2>
          <p className="env-section__body">
            {pending.map((p) => p.name).join(", ")} in <code>.axiom/providers/</code>. They are not used until you trust this folder: a
            provider decides which programs run to detect and install toolchains.
          </p>
          <Button data-hint="Use this folder's own providers" onClick={() => void store.trustWorkspace(true)}>
            Trust this folder's providers
          </Button>
        </section>
      )}

      {broken.length > 0 && (
        <section className="env-section" aria-label="Providers with errors">
          <h2 className="env-section__title">
            <Icon.Warning />
            Providers with errors
          </h2>
          <ul className="env-errors">
            {broken.map((p) => (
              <li key={`${p.source}:${p.id}`}>
                <span className="env-mono">
                  {p.id} ({p.source})
                </span>
                : {(p.errors ?? []).join("; ")}
              </li>
            ))}
          </ul>
        </section>
      )}

      <Dialog
        open={removing !== null}
        title={confirm ? `Remove ${confirm.name} ${confirm.removable}?` : "Remove"}
        description={
          confirm?.source ? (
            <>
              This deletes <code className="env-path">{shortenHome(confirm.source.where)}</code> and nothing else. Projects that select this
              version will need it installed again.
            </>
          ) : undefined
        }
        buttons={[
          { text: "Cancel", action: "cancel" },
          { text: "Remove", variant: "danger", action: "remove" },
        ]}
        dismissAction="cancel"
        testId="remove-toolchain"
        onChoose={(a) => {
          const r = removing;
          setRemoving(null);
          if (a === "remove" && r?.removable) void store.uninstall(r.providerId, r.removable);
        }}
      />
      <PlanDialog store={store} />
    </section>
  );
}

// ---- the install plan (M2-UI-002) -------------------------------------------------------

function fileName(url: string): string {
  return url.split("/").pop() || url;
}

/** One plan step as a list item: an icon per kind, a name, and the details the user approves. */
function StepItem({ step }: { step: Step }) {
  switch (step.kind) {
    case "download":
      return (
        <li className="plan-step" data-kind="download">
          <Icon.Download />
          <span className="plan-step__body">
            <span className="plan-step__name">
              Download {fileName(step.url)}
              {step.bytes != null && <span className="plan-step__meta"> · {formatBytes(step.bytes)}</span>}
            </span>
            <code className="plan-step__detail">{step.url}</code>
            <span className="plan-step__detail">
              SHA-256 {step.sha256.kind === "digest" ? <code>{step.sha256.value}</code> : <>from <code>{step.sha256.url}</code></>}, checked before
              anything is used
            </span>
          </span>
        </li>
      );
    case "extract":
      return (
        <li className="plan-step" data-kind="extract">
          <Icon.Archive />
          <span className="plan-step__body">
            <span className="plan-step__name">{step.format === "file" ? "Place the downloaded program" : `Extract the ${step.format} archive`}</span>
            <span className="plan-step__detail">Into Axiom's toolchains folder only</span>
          </span>
        </li>
      );
    case "run":
      return (
        <li className="plan-step" data-kind="run">
          <Icon.Terminal />
          <span className="plan-step__body">
            <span className="plan-step__name">Run {fileName(step.argv[0] ?? "")}</span>
            <code className="plan-step__detail">{step.argv.join(" ")}</code>
            {Object.entries(step.env ?? {}).map(([k, v]) => (
              <code key={k} className="plan-step__detail">
                {k}={v}
              </code>
            ))}
          </span>
        </li>
      );
    case "systemPackages":
      return (
        <li className="plan-step" data-kind="systemPackages">
          <Icon.Shield />
          <span className="plan-step__body">
            <span className="plan-step__name">
              Install system packages with {step.manager}: {step.packages.join(", ")}
            </span>
            <code className="plan-step__detail">{step.argv.join(" ")}</code>
          </span>
        </li>
      );
    case "verify":
      return (
        <li className="plan-step" data-kind="verify">
          <Icon.Check />
          <span className="plan-step__body">
            <span className="plan-step__name">Check that {step.toolId} runs</span>
          </span>
        </li>
      );
  }
}

/** What the plan does to the machine, in one sentence per effect. */
export function effectNotes(plan: InstallPlan): string[] {
  const out: string[] = [];
  if (plan.scope === "system") out.push("This changes your system outside Axiom's folders.");
  if (plan.effects.includes("elevation")) out.push("Your system will ask for an administrator password; Axiom never sees it.");
  if (plan.effects.includes("writeManaged") && plan.scope === "managed") out.push("Everything goes into Axiom's own toolchains folder and can be removed.");
  return out;
}

export function PlanDialog({ store }: { store: EnvironmentsStore }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  // The dialog keeps showing the last plan while it animates out.
  const last = useRef(s.plan);
  if (s.plan) last.current = s.plan;
  const shown = last.current;
  const plan = shown?.plan;
  return (
    <Dialog
      open={!!s.plan}
      title={plan ? `Install ${shown.providerName} ${plan.version}` : "Install"}
      buttons={[
        { text: "Cancel", action: "cancel" },
        { text: "Install", variant: "primary", action: "install" },
      ]}
      dismissAction="cancel"
      testId="install-plan"
      onChoose={(a) => {
        if (a === "install") void store.approvePlan();
        else store.dismissPlan();
      }}
    >
      {plan && (
        <div className="plan">
          {plan.scope === "system" && (
            <p className="plan__warning" role="note">
              <Icon.Warning />
              System install
            </p>
          )}
          {effectNotes(plan).map((n) => (
            <p key={n} className="plan__note">
              {n}
            </p>
          ))}
          <ol className="plan__steps" aria-label="Steps">
            {plan.steps.map((step, i) => (
              <StepItem key={i} step={step} />
            ))}
          </ol>
          {plan.downloadBytes != null && <p className="plan__note">Download size: {formatBytes(plan.downloadBytes)}</p>}
        </div>
      )}
    </Dialog>
  );
}

// ---- the side bar for the Environments destination ---------------------------------------

export function EnvironmentsSide({ store }: { store: EnvironmentsStore }) {
  /** Moves focus to that toolchain's row on the page (its action, else its name cell). */
  const showRow = (providerId: string) => {
    const row = document.querySelector<HTMLElement>(`.env-table [data-provider="${providerId}"]`);
    (row?.querySelector<HTMLElement>("button") ?? row)?.focus();
    row?.scrollIntoView({ block: "nearest" });
  };
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const rows = toolchainRows(s);
  if (rows.length === 0) {
    return <p className="tree-state tree-state__text--faint">{s.status === "loading" ? "Detecting…" : "No toolchains for this folder"}</p>;
  }
  return (
    <ul className="env-side" aria-label="Toolchains">
      {rows.map((row) => {
        const led = ledOf(row);
        return (
          <li key={row.providerId}>
            <button
              type="button"
              className="env-side__row"
              aria-label={`${row.name}: ${row.state.kind === "ready" ? `Ready, ${row.detected ?? ""}`.trim().replace(/,$/, "") : led.text}`}
              data-hint={`${row.name}: ${led.text}`}
              onClick={() => showRow(row.providerId)}
            >
              <Led state={led.state} />
              <span className="env-side__name">{row.name}</span>
              <span className="env-side__state">{row.state.kind === "ready" ? (row.detected ?? "") : led.text}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/** Rows that need the user (missing or failed): the rail's badge. */
export function attentionCount(store: EnvironmentsStore): number {
  return toolchainRows(store.getSnapshot()).filter((r) => r.state.kind === "missing" || r.state.kind === "error").length;
}
