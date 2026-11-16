// Measurement hooks for the benchmarks (bench/run.py, M1.6-QA-002). They exist
// only in a build made with `vite build --mode e2e` (pnpm run build:e2e): in
// any other mode `import.meta.env.MODE === "e2e"` is false at build time, the
// branch below is removed, and tests/probe-absent.test.ts fails if a trace of
// it reaches dist/.
//
// This module is imported FIRST by main.tsx: React DOM reads the DevTools global
// hook when it is first evaluated, so the commit counter has to be installed
// before it. (A <Profiler> would not do: its onRender never fires in a
// production React build.)
//
//   window.__axiomFrames.start() / .stop()   rAF intervals in ms between the two calls
//   window.__axiomMarks                      key -> first painted frame after a condition
//   window.__axiomCommits()                  React commits so far

interface Arm {
  /** The key that starts the clock has been seen. */
  started: boolean;
  /** performance.now()-based time at which the page received that key. */
  t0: number;
  finished: boolean;
  /** True once the awaited condition holds. */
  done: () => boolean;
}

if (import.meta.env.MODE === "e2e") {
  const w = window as unknown as Record<string, unknown>;

  // ---- React commit counter, through the DevTools hook production React calls
  let commits = 0;
  const renderers = new Map<number, unknown>();
  w.__REACT_DEVTOOLS_GLOBAL_HOOK__ ??= {
    supportsFiber: true,
    isDisabled: false,
    renderers,
    inject(renderer: unknown) {
      const id = renderers.size + 1;
      renderers.set(id, renderer);
      return id;
    },
    onCommitFiberRoot() {
      commits++;
    },
    onPostCommitFiberRoot() {},
    onCommitFiberUnmount() {},
    checkDCE() {},
  };
  w.__axiomCommits = () => commits;

  // ---- frame intervals
  let framing = false;
  let last = 0;
  let intervals: number[] = [];
  const tick = (now: number) => {
    if (!framing) return;
    if (last) intervals.push(Math.round((now - last) * 100) / 100);
    last = now;
    requestAnimationFrame(tick);
  };
  w.__axiomFrames = {
    start() {
      intervals = [];
      last = 0;
      framing = true;
      requestAnimationFrame(tick);
    },
    stop(): number[] {
      framing = false;
      return intervals;
    },
  };

  // ---- key -> first frame after a condition
  // expect(name, selector) / expectModelChange(name) arm one measurement per
  // call: the next keydown starts the clock (its own timeStamp, i.e. when the
  // page got the key) and the first animation frame after the condition becomes
  // true stops it. Later keys (Escape, arrows) start nothing.
  const arms = new Map<string, Arm>();
  const results = new Map<string, number[]>();
  const arm = (name: string, done: () => boolean) => {
    arms.set(name, { started: false, t0: 0, finished: false, done });
    if (!results.has(name)) results.set(name, []);
  };
  const settle = (name: string, a: Arm) => {
    if (!a.started || a.finished || !a.done()) return;
    a.finished = true;
    requestAnimationFrame(() => results.get(name)?.push(Math.round((performance.now() - a.t0) * 100) / 100));
  };
  const checkAll = () => arms.forEach((a, name) => settle(name, a));
  window.addEventListener(
    "keydown",
    (e) => {
      for (const a of arms.values()) {
        if (!a.started) {
          a.started = true;
          a.t0 = e.timeStamp;
        }
      }
      checkAll();
    },
    true,
  );
  new MutationObserver(checkAll).observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });

  let modelEvents = 0;
  let hooked = false;
  w.__axiomMarks = {
    /** The next key starts a measurement that ends at the first frame after `selector` matches. */
    expect(name: string, selector: string): void {
      arm(name, () => document.querySelector(selector) !== null);
    },
    /** The next key starts a measurement that ends at the first frame after Monaco swaps its model. */
    async expectModelChange(name: string): Promise<boolean> {
      if (!hooked) {
        hooked = true;
        const monaco = await import("monaco-editor");
        const hook = (ed: { onDidChangeModel: (f: () => void) => unknown }) => ed.onDidChangeModel(() => modelEvents++);
        monaco.editor.getEditors().forEach(hook);
        monaco.editor.onDidCreateEditor(hook);
      }
      const base = modelEvents;
      arm(name, () => modelEvents > base);
      // The model swap happens inside a React effect, not a DOM mutation: poll per frame.
      const a = arms.get(name)!;
      const poll = () => {
        settle(name, a);
        if (!a.finished && arms.get(name) === a) requestAnimationFrame(poll);
      };
      requestAnimationFrame(poll);
      return true;
    },
    /** The finished measurements for `name`, in milliseconds; clears them. */
    take(name: string): number[] {
      const out = results.get(name) ?? [];
      results.set(name, []);
      return out;
    },
  };
}

export {};
