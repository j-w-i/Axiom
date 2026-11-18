// The end-to-end crash test kills the webview's content process. That lookup is
// the only part of the suite that differs per OS, and two of the three can
// never be exercised from one machine, so it is unit-tested here against
// process tables shaped like each platform's real output:
//   Linux    `ps -e -o pid=,ppid=,args=` (observed on Fedora 44 / WebKitGTK 2.54)
//   macOS    the same `ps` format; WebKit's WebContent XPC service
//   Windows  Win32_Process (Name + CommandLine); every WebView2 process is
//            msedgewebview2.exe and only --type=renderer is a content process
// plus the failure modes that would kill the wrong process, or none.

import { describe, expect, it } from "vitest";
import {
  executableName,
  findDescendantsByName,
  findWebContentProcesses,
  isWebContentProcess,
  parsePsTable,
  parsePsTableWithArgs,
  parseWindowsTable,
  type Proc,
} from "../e2e/processes.mjs";

const APP = 1000;
const p = (pid: number, ppid: number, comm: string, args = ""): Proc => ({ pid, ppid, comm, args });

describe("command-line parsing", () => {
  it("takes the executable name from the first token, not the truncated comm", () => {
    expect(executableName("/usr/libexec/webkit2gtk-4.1/WebKitWebProcess 4 34")).toBe("WebKitWebProcess");
    expect(executableName("/usr/libexec/webkit2gtk-4.1/WebKitNetworkProcess 1 34")).toBe("WebKitNetworkProcess");
    expect(executableName("./target/debug/axiomd --data-dir /tmp/x")).toBe("axiomd");
    expect(
      executableName(
        "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent",
      ),
    ).toBe("com.apple.WebKit.WebContent");
    expect(executableName("[kworker/R-rcu_gp]")).toBe("[kworker/R-rcu_gp]");
  });

  it("parses `ps -o pid=,ppid=,args=` as observed on Linux", () => {
    const table = parsePsTableWithArgs(
      [
        " 691481       1 ./target/release/axiom-tauri --daemon ./target/debug/axiomd --data-dir /tmp/d",
        " 691519  691481 /usr/libexec/webkit2gtk-4.1/WebKitNetworkProcess 1 34",
        " 691524  691481 /usr/libexec/webkit2gtk-4.1/WebKitWebProcess 4 34",
        " 691525  691481 ./target/debug/axiomd --data-dir /tmp/tmp.gXPFaMXyTp",
        "      4       2 [kworker/R-rcu_gp]",
        "",
        "garbage",
      ].join("\n"),
    );
    expect(table.map((r) => [r.pid, r.ppid, r.comm])).toEqual([
      [691481, 1, "axiom-tauri"],
      [691519, 691481, "WebKitNetworkProcess"],
      [691524, 691481, "WebKitWebProcess"],
      [691525, 691481, "axiomd"],
      [4, 2, "[kworker/R-rcu_gp]"],
    ]);
  });

  it("still parses the legacy `pid ppid comm` form", () => {
    expect(parsePsTable(" 7 1 init\nbad\n")).toEqual([{ pid: 7, ppid: 1, comm: "init", args: "" }]);
  });

  it("parses Windows CIM output with command lines, as an array and as one object", () => {
    const many = parseWindowsTable(
      JSON.stringify([
        { ProcessId: 1000, ParentProcessId: 1, Name: "axiom-tauri.exe", CommandLine: "axiom-tauri.exe" },
        {
          ProcessId: 1001,
          ParentProcessId: 1000,
          Name: "msedgewebview2.exe",
          CommandLine: '"C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\msedgewebview2.exe" --type=renderer --x',
        },
      ]),
    );
    expect(many.map((r) => [r.pid, r.comm])).toEqual([
      [1000, "axiom-tauri.exe"],
      [1001, "msedgewebview2.exe"],
    ]);
    expect(many[1]!.args).toContain("--type=renderer");
    const one = parseWindowsTable(JSON.stringify({ ProcessId: 5, ParentProcessId: 6, Name: "x.exe" }));
    expect(one).toEqual([{ pid: 5, ppid: 6, comm: "x.exe", args: "" }]);
    expect(parseWindowsTable("")).toEqual([]);
  });
});

describe("what counts as a webview content process", () => {
  it("Linux: WebKitWebProcess yes, WebKitNetworkProcess no", () => {
    expect(isWebContentProcess(p(1, APP, "WebKitWebProcess"))).toBe(true);
    expect(isWebContentProcess(p(1, APP, "WebKitNetworkProcess"))).toBe(false);
  });

  it("macOS: the WebContent service yes, the Networking service no", () => {
    expect(isWebContentProcess(p(1, APP, "com.apple.WebKit.WebContent"))).toBe(true);
    expect(isWebContentProcess(p(1, APP, "com.apple.WebKit.Networking"))).toBe(false);
  });

  it("Windows: only the WebView2 renderer, never the browser, GPU or utility process", () => {
    const wv = (args: string) => p(1, APP, "msedgewebview2.exe", args);
    expect(isWebContentProcess(wv("msedgewebview2.exe --type=renderer --lang=en"))).toBe(true);
    expect(isWebContentProcess(wv("msedgewebview2.exe --embedded-browser-webview=1"))).toBe(false);
    expect(isWebContentProcess(wv("msedgewebview2.exe --type=gpu-process"))).toBe(false);
    expect(isWebContentProcess(wv("msedgewebview2.exe --type=utility --utility-sub-type=x"))).toBe(false);
    expect(isWebContentProcess(wv("msedgewebview2.exe --type=crashpad-handler"))).toBe(false);
  });
});

describe("finding the app's content process", () => {
  it("Linux (from the observed process table)", () => {
    const table = parsePsTableWithArgs(
      [
        " 1000    1 ./target/release/axiom-tauri",
        " 1001 1000 /usr/libexec/webkit2gtk-4.1/WebKitNetworkProcess 1 34",
        " 1002 1000 /usr/libexec/webkit2gtk-4.1/WebKitWebProcess 4 34",
        " 1003 1000 ./target/debug/axiomd --data-dir /tmp/d",
      ].join("\n"),
    );
    expect(findWebContentProcesses(table, APP)).toEqual([1002]);
  });

  it("Windows: the renderer under the WebView2 browser process, not the browser itself", () => {
    const table = [
      p(APP, 1, "axiom-tauri.exe"),
      p(2001, APP, "msedgewebview2.exe", "msedgewebview2.exe --embedded-browser-webview=1"),
      p(2002, 2001, "msedgewebview2.exe", "msedgewebview2.exe --type=gpu-process"),
      p(2003, 2001, "msedgewebview2.exe", "msedgewebview2.exe --type=renderer"),
      p(2004, 2001, "msedgewebview2.exe", "msedgewebview2.exe --type=utility"),
    ];
    expect(findWebContentProcesses(table, APP)).toEqual([2003]);
  });

  it("macOS: WebContent is launched by launchd, so it is not a descendant and is not found", () => {
    // WKWebView's XPC services are children of launchd (pid 1), not of the app.
    // A descendant-only lookup therefore finds nothing on macOS - by design: it
    // must never pick another app's WebContent. The macOS crash check uses the
    // engine's own termination report instead (e2e/smoke.mjs, check 11).
    const table = [p(APP, 1, "axiom-tauri"), p(3001, 1, "com.apple.WebKit.WebContent")];
    expect(findWebContentProcesses(table, APP)).toEqual([]);
  });

  it("never returns another application's webview", () => {
    const table = [
      p(APP, 1, "axiom-tauri"),
      p(4001, 1, "WebKitWebProcess"),
      p(4002, 5000, "msedgewebview2.exe", "msedgewebview2.exe --type=renderer"),
    ];
    expect(findWebContentProcesses(table, APP)).toEqual([]);
  });

  it("finds this app's daemon and not another instance's", () => {
    const table = [p(APP, 1, "axiom-tauri"), p(7001, APP, "axiomd"), p(7002, 8888, "axiomd")];
    expect(findDescendantsByName(table, APP, /^axiomd(\.exe)?$/i)).toEqual([7001]);
  });

  it("stops walking on a cycle instead of looping forever", () => {
    const table = [p(APP, 1, "axiom-tauri"), p(6001, 6002, "WebKitWebProcess"), p(6002, 6001, "x")];
    expect(findWebContentProcesses(table, APP)).toEqual([]);
  });
});
