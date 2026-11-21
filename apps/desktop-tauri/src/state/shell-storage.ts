// Shell-local preferences (layout, palette history, the chosen model) that must survive a quit.
// They live in `localStorage` for synchronous reads, and every write also goes to the host, which
// keeps them on disk before the call returns. WebKit writes `localStorage` to disk on a timer and
// loses what was written in the second before the app quits (D-X-2, verification/M1.6.md), so at
// startup the host's copy is put back into `localStorage` before any store reads it.

export type KeyValue = Pick<Storage, "getItem" | "setItem" | "removeItem">;

/** The host side (`host.localAll` / `host.localSet` in the app; absent in unit tests). */
export interface LocalHost {
  localAll(): Promise<Record<string, string>>;
  localSet(key: string, value: string | null): Promise<void>;
}

/** An in-memory stand-in where `localStorage` is missing or throws (tests, private modes). */
export function memoryStorage(): KeyValue {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, String(v)),
    removeItem: (k) => void m.delete(k),
  };
}

function defaultLocal(): KeyValue {
  try {
    if (typeof localStorage !== "undefined") return localStorage;
  } catch {
    /* access denied */
  }
  return memoryStorage();
}

/** Write-through storage: reads from `local`; writes to `local` and to the host. */
export class ShellStorage implements KeyValue {
  private host: LocalHost | null = null;

  constructor(private readonly local: KeyValue = defaultLocal()) {}

  getItem(key: string): string | null {
    try {
      return this.local.getItem(key);
    } catch {
      return null;
    }
  }

  setItem(key: string, value: string): void {
    try {
      this.local.setItem(key, value);
    } catch {
      /* storage full: the host still keeps it */
    }
    void this.host?.localSet(key, value).catch(() => {});
  }

  removeItem(key: string): void {
    try {
      this.local.removeItem(key);
    } catch {
      /* unavailable */
    }
    void this.host?.localSet(key, null).catch(() => {});
  }

  /** Puts the host's copy back into `local` (the host's is the durable one) and sends later
   * writes to the host. Bounded: a host that does not answer within `timeoutMs` leaves `local`
   * as it is, so startup never waits on it. */
  async attach(host: LocalHost, timeoutMs = 500): Promise<void> {
    this.host = host;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const all = await Promise.race([
      host.localAll().catch(() => null),
      new Promise<null>((r) => (timer = setTimeout(() => r(null), timeoutMs))),
    ]);
    clearTimeout(timer);
    if (!all) return;
    for (const [k, v] of Object.entries(all)) {
      try {
        this.local.setItem(k, v);
      } catch {
        /* storage full */
      }
    }
  }
}

/** The app's instance; `main.tsx` attaches the host before building the stores. */
export const shellStorage = new ShellStorage();
