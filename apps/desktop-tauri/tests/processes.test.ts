// The end-to-end crash test kills the webview's content process. That logic is
// the only part of the E2E suite that differs per OS, and it can never be
// exercised on the other two platforms from one machine — so it is unit-tested
// here against synthetic process tables from all three, including real `ps`
// quirks (15-character `comm` truncation) and the failure modes that would
// otherwise silently kill the wrong process or none at all.

import { describe, expect, it } from "vitest";
import {
  findDescendantsByName,
  findWebContentProcesses,
  parsePsTable,
  parseWindowsTable,
  type Proc,
} from "../e2e/processes.mjs";

const APP = 1000;

const p = (pid: number, ppid: number, comm: string): Proc => ({ pid, ppid, comm });

describe("process table parsing", () => {
  it("parses ps output, including truncated comm names and spaces", () => {
    const table = parsePsTable(
      [
        " 1000  999 axiom-tauri",
        " 1001 1000 WebKitWebProces",
        " 1002 1000 WebKitNetworkPr",
        " 1003 1000 axiomd",
        " 1004  1000 Google Chrome Helper (Renderer)",
        "",
      ].join("\n"),
    );
    expect(table).toHaveLength(5);
    expect(table[1]).toEqual({ pid: 1001, ppid: 1000, comm: "WebKitWebProces" });
    expect(table[4]!.comm).toBe("Google Chrome Helper (Renderer)");
  });

  it("skips unparseable ps lines instead of inventing processes", () => {
    const table = parsePsTable("garbage\n\n 7 1 init\n");
    expect(table).toEqual([{ pid: 7, ppid: 1, comm: "init" }]);
  });

  it("parses Windows CIM output as an array and as a single object", () => {
    const many = parseWindowsTable(
      JSON.stringify([
        { ProcessId: 1000, ParentProcessId: 1, Name: "axiom-tauri.exe" },
        { ProcessId: 1001, ParentProcessId: 1000, Name: "msedgewebview2.exe" },
      ]),
    );
    expect(many.map((r) => r.pid)).toEqual([1000, 1001]);
    const one = parseWindowsTable(JSON.stringify({ ProcessId: 5, ParentProcessId: 6, Name: "x.exe" }));
    expect(one).toEqual([{ pid: 5, ppid: 6, comm: "x.exe" }]);
    expect(parseWindowsTable("")).toEqual([]);
  });
});

describe("finding the webview content process", () => {
  it("finds WebKitWebProcess on Linux (truncated comm)", () => {
    const table = [
      p(APP, 1, "axiom-tauri"),
      p(1001, APP, "WebKitWebProces"),
      p(1002, APP, "WebKitNetworkPr"),
      p(1003, APP, "axiomd"),
    ];
    expect(findWebContentProcesses(table, APP)).toEqual([1001]);
  });

  it("finds msedgewebview2.exe on Windows", () => {
    const table = [
      p(APP, 1, "axiom-tauri.exe"),
      p(2001, APP, "msedgewebview2.exe"),
      p(2002, 2001, "msedgewebview2.exe"),
    ];
    expect(findWebContentProcesses(table, APP)).toEqual([2001, 2002]);
  });

  it("finds a WebContent process on macOS through one intermediate process", () => {
    const table = [p(APP, 1, "axiom-tauri"), p(3001, APP, "launcher"), p(3002, 3001, "com.apple.WebKit.WebContent")];
    expect(findWebContentProcesses(table, APP)).toEqual([3002]);
  });

  it("never returns another application's webview", () => {
    const table = [
      p(APP, 1, "axiom-tauri"),
      p(4001, 1, "WebKitWebProces"), // unrelated, reparented to init
      p(4002, 5000, "msedgewebview2.exe"), // another app entirely
    ];
    expect(findWebContentProcesses(table, APP)).toEqual([]);
  });

  it("ignores the app's own process and its daemon", () => {
    const table = [p(APP, 1, "axiom-tauri"), p(5001, APP, "axiomd")];
    expect(findWebContentProcesses(table, APP)).toEqual([]);
  });

  it("finds this app's daemon and not another instance's", () => {
    const table = [
      p(APP, 1, "axiom-tauri"),
      p(7001, APP, "axiomd"),
      p(7002, 8888, "axiomd"), // a different Axiom (or a developer's terminal)
    ];
    expect(findDescendantsByName(table, APP, /^axiomd/i)).toEqual([7001]);
  });

  it("stops walking on a cycle instead of looping forever", () => {
    const table = [p(APP, 1, "axiom-tauri"), p(6001, 6002, "WebKitWebProces"), p(6002, 6001, "x")];
    expect(findWebContentProcesses(table, APP)).toEqual([]);
  });
});
