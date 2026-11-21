import { describe, expect, it } from "vitest";
import { memoryStorage, ShellStorage, type LocalHost } from "../src/state/shell-storage";

function fakeHost(initial: Record<string, string> = {}) {
  const map = { ...initial };
  const calls: [string, string | null][] = [];
  const host: LocalHost = {
    localAll: async () => ({ ...map }),
    localSet: async (k, v) => {
      calls.push([k, v]);
      if (v === null) delete map[k];
      else map[k] = v;
    },
  };
  return { host, map, calls };
}

describe("shell-local preferences survive a quit (D-X-2)", () => {
  it("restores the host's copy over a stale localStorage before anything reads it", async () => {
    const local = memoryStorage();
    local.setItem("axiom.layout.v2", '{"sidebar":252}');
    const s = new ShellStorage(local);
    const { host } = fakeHost({ "axiom.layout.v2": '{"sidebar":312}', "axiom.palette.mru.v1": '["a"]' });
    await s.attach(host);
    expect(s.getItem("axiom.layout.v2")).toBe('{"sidebar":312}');
    expect(s.getItem("axiom.palette.mru.v1")).toBe('["a"]');
  });

  it("writes through to the host, including removals", async () => {
    const s = new ShellStorage(memoryStorage());
    const { host, map, calls } = fakeHost();
    await s.attach(host);
    s.setItem("k", "v");
    s.removeItem("gone");
    await Promise.resolve();
    expect(calls).toEqual([
      ["k", "v"],
      ["gone", null],
    ]);
    expect(map).toEqual({ k: "v" });
    expect(s.getItem("k")).toBe("v");
  });

  it("does not wait for a host that does not answer, and keeps working without one", async () => {
    const s = new ShellStorage(memoryStorage());
    const t0 = Date.now();
    await s.attach({ localAll: () => new Promise(() => {}), localSet: async () => {} }, 50);
    expect(Date.now() - t0).toBeLessThan(1000);
    s.setItem("k", "v");
    expect(s.getItem("k")).toBe("v");
    const failing = new ShellStorage(memoryStorage());
    await failing.attach({ localAll: () => Promise.reject(new Error("no host")), localSet: () => Promise.reject(new Error("no host")) });
    failing.setItem("x", "y");
    expect(failing.getItem("x")).toBe("y");
  });
});
