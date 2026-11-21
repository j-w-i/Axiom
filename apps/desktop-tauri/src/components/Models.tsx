// Model providers (M3-UI-003; ADR-0017 §5; workspace-spec.md §5.4 first run, §4 Settings):
// the first-run setup shown in Chat when no model is available, and the Models pane of
// Settings. Keys go to the OS credential store through the daemon; the field clears on
// success and the UI only ever shows whether a key is stored.

import { useState, useSyncExternalStore } from "react";
import type { ProviderConfig } from "../daemon/protocol.gen";
import { providerId, type AgentStore, type ProviderOption } from "../state/agent";
import * as Icon from "../ui/icons";
import { Menu, type MenuEntry } from "../ui/overlays";
import { Button, Field, Segmented, Spinner } from "../ui/primitives";
import "../styles/components/models.css";

type Kind = "anthropic" | "openai";

const KIND_LABEL: Record<string, string> = {
  anthropic: "Anthropic",
  openaiChat: "OpenAI-compatible",
  acp: "Agent",
  scripted: "Test",
};

/** The configuration a filled form describes, or an error sentence. Pure (exported for tests). */
export function formConfig(kind: Kind, baseUrl: string, name: string, existing: string[]): ProviderConfig | string {
  const url = baseUrl.trim();
  if (kind === "anthropic") {
    const id = existing.includes("anthropic") && url ? providerId(url) : "anthropic";
    return { id, kind: "anthropic", name: name.trim() || "Anthropic", ...(url ? { baseUrl: url } : {}) };
  }
  if (url && !/^https?:\/\//i.test(url)) return "The server address starts with https:// (or http:// for this computer).";
  const effective = url || "https://api.openai.com/v1";
  const official = /^https:\/\/api\.openai\.com/i.test(effective);
  const id = official ? "openai" : providerId(effective.replace(/\/v1\/?$/, ""));
  return { id, kind: "openaiChat", name: name.trim() || (official ? "OpenAI" : effective.replace(/^https?:\/\//, "").replace(/\/v1\/?$/, "")), ...(url ? { baseUrl: url } : {}) };
}

/** The add-a-provider form: kind, server (OpenAI-compatible), key. */
function AddProvider({ store, compact = false }: { store: AgentStore; compact?: boolean }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const [kind, setKind] = useState<Kind>("anthropic");
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const save = async () => {
    const c = formConfig(kind, url, name, s.providers.map((p) => p.providerId));
    if (typeof c === "string") {
      setError(c);
      return;
    }
    if (kind === "anthropic" && !key.trim()) {
      setError("Anthropic needs an API key.");
      return;
    }
    setBusy(true);
    setError("");
    const err = await store.configure(c, key.trim() || undefined);
    setBusy(false);
    // The key leaves the UI either way: it is in the credential store or it was refused.
    setKey("");
    if (err) setError(err);
    else {
      setUrl("");
      setName("");
    }
  };
  return (
    <form
      className="models-add"
      aria-label="Add a model provider"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <Segmented
        label="Provider kind"
        value={kind}
        options={[
          { value: "anthropic", label: "Anthropic" },
          { value: "openai", label: "OpenAI-compatible" },
        ]}
        onChange={(v) => {
          setKind(v);
          setError("");
        }}
      />
      {kind === "openai" && (
        <label className="models-add__row">
          <span>Server</span>
          <Field mono aria-label="Server address" placeholder="https://api.openai.com/v1" value={url} onChange={(e) => setUrl(e.target.value)} data-hint="An OpenAI-compatible API: OpenAI, a hosted service, or a server on this computer (http://127.0.0.1…)" />
        </label>
      )}
      <label className="models-add__row">
        <span>API key</span>
        <Field
          type="password"
          autoComplete="off"
          aria-label="API key"
          placeholder={kind === "openai" ? "Not needed for a server on this computer" : "sk-ant-…"}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          data-hint="Stored in the system keychain, never in Axiom's settings"
        />
      </label>
      {!compact && (
        <label className="models-add__row">
          <span>Name</span>
          <Field aria-label="Provider name" placeholder="Optional" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
      )}
      {error && (
        <p className="models__error" role="alert">
          {error}
        </p>
      )}
      <div className="models-add__actions">
        {/* Primary only in the first-run panel: Settings has its own primary (Done). */}
        <Button variant={compact ? "primary" : "default"} type="submit" loading={busy} data-testid="models-save">
          Add provider
        </Button>
      </div>
    </form>
  );
}

function LocalServers({ store }: { store: AgentStore }) {
  const [busy, setBusy] = useState(false);
  const [found, setFound] = useState<{ name: string; found: boolean; detail: string }[] | null>(null);
  return (
    <div className="models-local">
      <Button
        loading={busy}
        onClick={() => {
          setBusy(true);
          void store.detectLocal().then((r) => {
            setFound(r);
            setBusy(false);
          });
        }}
        data-hint="Looks for Ollama, llama.cpp and LM Studio on their usual ports"
        data-testid="models-detect"
      >
        Find local servers
      </Button>
      {found && (
        <ul className="models-local__list" aria-label="Local servers">
          {found.map((f) => (
            <li key={f.name}>
              {f.found ? <Icon.Check /> : <Icon.Close />} {f.name}: {f.detail}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Agents({ providers, hasWorkspace }: { providers: ProviderOption[]; hasWorkspace: boolean }) {
  const agents = providers.filter((p) => p.kind === "acp");
  if (agents.length === 0) return null;
  return (
    <ul className="models-agents" aria-label="Agents">
      {agents.map((a) => (
        <li key={a.providerId} className="models-agents__row">
          <span className="models-agents__name">{a.name}</span>
          <span className="models-agents__state">
            {a.error ? a.error : hasWorkspace ? "Ready: choose it in a folder's chat" : "Ready: open a folder to use it"}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** Chat's first-run setup (ADR-0017 §5): shown when no model is available. */
export function ModelSetup({ store, hasWorkspace }: { store: AgentStore; hasWorkspace: boolean }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  return (
    <div className="models-setup" data-testid="model-setup">
      <h3 className="models-setup__title">Connect a model to start</h3>
      <p className="models-setup__line">Axiom talks to the model you choose. Pick one way; you can add more in Settings → Models.</p>
      <section className="models-setup__card" aria-labelledby="setup-key">
        <h4 id="setup-key">With an API key or a server</h4>
        <AddProvider store={store} compact />
      </section>
      <section className="models-setup__card" aria-labelledby="setup-local">
        <h4 id="setup-local">A model on this computer</h4>
        <p className="models-setup__hint">Ollama, llama.cpp or LM Studio, running on their usual ports.</p>
        <LocalServers store={store} />
      </section>
      {s.providers.some((p) => p.kind === "acp") && (
        <section className="models-setup__card" aria-labelledby="setup-agent">
          <h4 id="setup-agent">An agent you already use</h4>
          <p className="models-setup__hint">Claude Code, Codex or Gemini CLI, with your own sign-in. They work in a folder.</p>
          <Agents providers={s.providers} hasWorkspace={hasWorkspace} />
        </section>
      )}
    </div>
  );
}

function ProviderRow({ p, store }: { p: ProviderOption; store: AgentStore }) {
  const [editing, setEditing] = useState(false);
  const [key, setKey] = useState("");
  const [error, setError] = useState("");
  const keyState = p.apiKeyEnv ? `Key from $${p.apiKeyEnv}` : p.hasKey ? "Key stored" : "No key";
  return (
    <li className="models-row" data-provider={p.providerId}>
      <div className="models-row__main">
        <span className="models-row__name">{p.name}</span>
        <span className="models-row__kind">{KIND_LABEL[p.kind] ?? p.kind}</span>
        {p.baseUrl && <span className="models-row__url">{p.baseUrl}</span>}
      </div>
      <div className="models-row__state">
        {p.kind !== "acp" && <span data-testid="key-state">{keyState}</span>}
        <span className={p.error ? "models-row__err" : undefined}>{p.error ? p.error : `${p.models.length} model${p.models.length === 1 ? "" : "s"}`}</span>
      </div>
      {!p.builtin && p.kind !== "scripted" && (
        <div className="models-row__actions">
          {p.kind !== "acp" && !p.apiKeyEnv && (
            <>
              <Button size="sm" onClick={() => setEditing(!editing)} aria-expanded={editing}>
                {p.hasKey ? "Replace key" : "Set key"}
              </Button>
              {p.hasKey && (
                <Button size="sm" variant="quiet" onClick={() => void store.setKey(p.providerId, null).then(setError)}>
                  Clear key
                </Button>
              )}
            </>
          )}
          <Button size="sm" variant="quiet" onClick={() => void store.removeProvider(p.providerId).then(setError)} aria-label={`Remove ${p.name}`}>
            Remove
          </Button>
        </div>
      )}
      {editing && (
        <form
          className="models-row__key"
          onSubmit={(e) => {
            e.preventDefault();
            const k = key;
            setKey("");
            void store.setKey(p.providerId, k).then((err) => {
              setError(err);
              if (!err) setEditing(false);
            });
          }}
        >
          <Field type="password" autoComplete="off" aria-label={`API key for ${p.name}`} value={key} onChange={(e) => setKey(e.target.value)} autoFocus />
          <Button size="sm" variant="primary" type="submit" disabled={!key.trim()}>
            Save key
          </Button>
        </form>
      )}
      {error && (
        <p className="models__error" role="alert">
          {error}
        </p>
      )}
    </li>
  );
}

/** The Models pane of Settings. */
export function ModelsPane({ store, hasWorkspace }: { store: AgentStore; hasWorkspace: boolean }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot);
  if (s.status === "idle") void store.load();
  const configured = s.providers.filter((p) => !p.builtin && p.kind !== "acp");
  const usable = s.providers.filter((p) => !p.error && p.models.length && p.kind !== "acp");
  const current = s.model ? usable.find((p) => p.providerId === s.model?.providerId) : undefined;
  const currentName = current ? `${current.name} · ${current.models.find((m) => m.id === s.model?.model)?.name ?? s.model?.model}` : "None";
  const items: MenuEntry[] = usable.flatMap((p) => [
    { heading: p.name } as MenuEntry,
    ...p.models.map((m) => ({ label: m.name, onSelect: () => store.setModel({ providerId: p.providerId, model: m.id }) })),
  ]);
  return (
    <div className="models-pane" data-testid="models-pane">
      <div className="setting">
        <div className="setting__text">
          <span className="setting__label">Default model</span>
          <span className="setting__desc">For new chats.</span>
        </div>
        {items.length ? <Menu label={`Default model: ${currentName}`} trigger={<span>{currentName}</span>} items={items} /> : <span className="setting__desc">Add a provider first.</span>}
      </div>
      <h3 className="models-pane__heading">Providers</h3>
      {s.status === "loading" && (
        <p className="setting__desc">
          <Spinner /> Checking providers…
        </p>
      )}
      {configured.length === 0 ? <p className="setting__desc">None yet.</p> : <ul className="models-list">{configured.map((p) => <ProviderRow key={p.providerId} p={p} store={store} />)}</ul>}
      <h3 className="models-pane__heading">Add a provider</h3>
      <AddProvider store={store} />
      <h3 className="models-pane__heading">On this computer</h3>
      <LocalServers store={store} />
      <h3 className="models-pane__heading">Agents</h3>
      <Agents providers={s.providers} hasWorkspace={hasWorkspace} />
    </div>
  );
}
