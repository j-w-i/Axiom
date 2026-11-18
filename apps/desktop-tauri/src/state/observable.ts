// Minimal external store for React's useSyncExternalStore. State objects are
// replaced (never mutated in place) so snapshots compare by identity.

export class Observable<S extends object> {
  private readonly listeners = new Set<() => void>();

  constructor(protected state: S) {}

  readonly getSnapshot = (): S => this.state;

  readonly subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  protected set(patch: Partial<S>): void {
    this.state = { ...this.state, ...patch };
    for (const fn of [...this.listeners]) fn();
  }
}

/** Single-shot debounce with flush, like the Qt models' QTimer(singleShot). */
export class Debounce {
  private timer: ReturnType<typeof setTimeout> | null = null;
  constructor(
    private readonly ms: number,
    private readonly fn: () => void,
  ) {}
  schedule(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.fn();
    }, this.ms);
  }
  stop(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
  get pending(): boolean {
    return this.timer !== null;
  }
}
