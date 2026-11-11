// Browser dev harness (`pnpm dev`): when there is no Qt, install a fake
// `host` that logs slot calls and opens a sample document after `ready`.

import type { Host, HostSignals, ReadyPayload } from "./bridge";

type Listener = (payload: never) => void;

export interface FakeHost {
  host: Host;
  /** Emit a shell -> page signal, as the real shell would. */
  emit<K extends keyof HostSignals>(name: K, payload: Parameters<Parameters<HostSignals[K]["connect"]>[0]>[0]): void;
}

const SIGNALS = [
  "configure",
  "openDocument",
  "activateDocument",
  "closeDocument",
  "requestSave",
  "documentSaved",
  "saveFailed",
  "revealPosition",
  "runCommand",
] as const;

export function createFakeHost(log: (line: string) => void = (l) => console.log(l)): FakeHost {
  const listeners = new Map<string, Listener[]>();
  const signal = (name: string) => ({
    connect(fn: Listener) {
      const list = listeners.get(name) ?? [];
      list.push(fn);
      listeners.set(name, list);
    },
  });
  const emit = (name: string, payload: unknown) => {
    for (const fn of listeners.get(name) ?? []) (fn as (p: unknown) => void)(payload);
  };
  const slot = (name: string) => (payload: unknown) => {
    log(`[host.${name}] ${JSON.stringify(payload)}`);
  };

  const signals = Object.fromEntries(SIGNALS.map((n) => [n, signal(n)])) as unknown as HostSignals;
  const host: Host = {
    ...signals,
    ready(payload: ReadyPayload) {
      slot("ready")(payload);
      // The shell answers `ready` with configure, then re-opens documents.
      queueMicrotask(() => {
        emit("configure", { theme: "dark", fontSize: 14, fontFamily: "", wordWrap: false });
        emit("openDocument", {
          path: "src/sample.ts",
          text: SAMPLE,
          version: "dev",
          activate: true,
          dirty: false,
        });
        emit("activateDocument", { path: "src/sample.ts" });
      });
    },
    saveRequested(payload) {
      slot("saveRequested")({ path: payload.path, textLength: payload.text.length });
      // Pretend the shell wrote the file.
      setTimeout(() => emit("documentSaved", { path: payload.path, version: "dev" }), 100);
    },
    dirtyChanged: slot("dirtyChanged"),
    cursorChanged: slot("cursorChanged"),
    textSnapshot(payload) {
      slot("textSnapshot")({ path: payload.path, textLength: payload.text.length });
    },
    openLink: slot("openLink"),
    reportError: slot("reportError"),
  };
  return { host, emit: emit as FakeHost["emit"] };
}

const SAMPLE = `// Axiom editor dev harness. Edit me, press Ctrl+S, watch the console.
export function greet(name: string): string {
  return "hello, " + name;
}

const answer: number = "forty-two"; // a type error, to show the TS worker runs
console.log(greet("axiom"), answer);
`;
