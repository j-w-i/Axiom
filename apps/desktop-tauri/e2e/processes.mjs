// Finding (and killing) a webview's content process, portably.
//
// The end-to-end crash test (T-08) must kill the process that renders the page:
//   Linux    WebKitWebProcess (`ps` shows it as "WebKitWebProces": 15 chars)
//   Windows  a WebView2 *renderer*: msedgewebview2.exe with --type=renderer.
//            Every WebView2 process has that executable name (browser, GPU,
//            utility, renderer); killing the browser process would test a
//            different failure, so the command line decides.
//            https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model
//   macOS    com.apple.WebKit.WebContent
// This module keeps the decisions pure and testable (`findWebContentProcesses`)
// and isolates the one platform call (`readProcessTable`), so
// `tests/processes.test.ts` covers all three OSes from any machine.

import { execFileSync } from "node:child_process";

/**
 * @typedef {{ pid: number, ppid: number, comm: string, args: string }} Proc
 */

/** Linux and macOS content-process names (full names: they come from the
 * executable path, not the truncated `comm`). */
const POSIX_CONTENT = /^WebKitWebProcess$|^com\.apple\.WebKit\.WebContent$/i;
/** Windows: the WebView2 executable; only its renderer is a content process. */
const WEBVIEW2 = /^msedgewebview2(\.exe)?$/i;

/**
 * Parses `ps -e -o pid=,ppid=,comm=,args=`-style output (Linux and macOS):
 * pid, ppid, a single-token comm, then the full command line. Lines that do not
 * parse are skipped rather than throwing: `ps` output is not a contract, and a
 * missing row must never silently become process 0.
 *
 * Older callers pass only `pid ppid comm` (no args); a multi-word comm without
 * args is kept whole.
 *
 * @param {string} text
 * @returns {Proc[]}
 */
export function parsePsTable(text) {
  const out = [];
  for (const line of text.split("\n")) {
    const m = /^\s*(\d+)\s+(\d+)\s+(.+?)\s*$/.exec(line);
    if (!m) continue;
    out.push({ pid: Number(m[1]), ppid: Number(m[2]), comm: m[3], args: "" });
  }
  return out;
}

/**
 * The executable's file name from a command line: the last path component of
 * the first whitespace-separated token. A kernel thread's `[name]` is returned
 * unchanged.
 *
 * Process names are derived this way rather than read from `comm`/`ucomm`,
 * because those are truncated on Linux (15 characters) and may contain spaces
 * on macOS, which makes them unparseable when another column follows.
 *
 * Limitation (deliberate): an executable whose *path* contains a space is cut
 * at that space. None of the processes this module looks for has one:
 * /usr/libexec/webkit2gtk-4.1/WebKitWebProcess,
 * /System/Library/Frameworks/WebKit.framework/.../com.apple.WebKit.WebContent,
 * and on Windows the name comes from Win32_Process.Name, not from here.
 *
 * @param {string} args
 * @returns {string}
 */
export function executableName(args) {
  const trimmed = args.trim();
  if (trimmed.startsWith("[")) return trimmed;
  const exe = trimmed.split(/\s+/)[0] ?? "";
  const parts = exe.split(/[\\/]/);
  return parts[parts.length - 1] ?? "";
}

/**
 * Parses `ps -e -o pid=,ppid=,args=` output (Linux and macOS): two numeric
 * columns, then the full command line. Lines that do not parse are skipped.
 *
 * @param {string} text
 * @returns {Proc[]}
 */
export function parsePsTableWithArgs(text) {
  const out = [];
  for (const line of text.split("\n")) {
    const m = /^\s*(\d+)\s+(\d+)\s+(.*?)\s*$/.exec(line);
    if (!m) continue;
    const args = m[3] ?? "";
    out.push({ pid: Number(m[1]), ppid: Number(m[2]), comm: executableName(args), args });
  }
  return out;
}

/**
 * Parses `Get-CimInstance Win32_Process | ConvertTo-Json` output (Windows).
 * PowerShell emits a single object when there is one process and an array
 * otherwise; both are accepted.
 *
 * @param {string} text
 * @returns {Proc[]}
 */
export function parseWindowsTable(text) {
  const trimmed = text.trim();
  if (!trimmed) return [];
  const parsed = JSON.parse(trimmed);
  const rows = Array.isArray(parsed) ? parsed : [parsed];
  return rows
    .filter((r) => r && typeof r.ProcessId === "number")
    .map((r) => ({
      pid: r.ProcessId,
      ppid: typeof r.ParentProcessId === "number" ? r.ParentProcessId : 0,
      comm: String(r.Name ?? ""),
      args: String(r.CommandLine ?? ""),
    }));
}

/** Reads the live process table of this machine. */
export function readProcessTable() {
  if (process.platform === "win32") {
    const json = execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine | ConvertTo-Json -Compress",
      ],
      { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    );
    return parseWindowsTable(json);
  }
  const ps = execFileSync("ps", ["-e", "-o", "pid=,ppid=,args="], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return parsePsTableWithArgs(ps);
}

/**
 * Processes for which `match(proc)` holds **and** that are descendants of
 * `appPid`. Descendants only: another application's process must never be
 * touched, which is also why there is no "fall back to any match".
 *
 * @param {Proc[]} table
 * @param {number} appPid
 * @param {(p: Proc) => boolean} match
 * @returns {number[]}
 */
export function findDescendants(table, appPid, match) {
  const byPid = new Map(table.map((p) => [p.pid, p]));
  const isDescendant = (pid) => {
    let p = byPid.get(pid);
    for (let i = 0; i < 16 && p && p.ppid > 1; i++) {
      if (p.ppid === appPid) return true;
      p = byPid.get(p.ppid);
    }
    return false;
  };
  return table.filter((p) => match(p) && isDescendant(p.pid)).map((p) => p.pid);
}

/**
 * Descendants of `appPid` whose name matches `name`.
 *
 * @param {Proc[]} table
 * @param {number} appPid
 * @param {RegExp} name
 * @returns {number[]}
 */
export function findDescendantsByName(table, appPid, name) {
  return findDescendants(table, appPid, (p) => name.test(p.comm));
}

/**
 * Is this a webview *content* (renderer) process?
 *
 * @param {Proc} p
 * @returns {boolean}
 */
export function isWebContentProcess(p) {
  if (WEBVIEW2.test(p.comm)) return /--type=renderer(\s|$)/.test(p.args ?? "");
  return POSIX_CONTENT.test(p.comm);
}

/**
 * The content processes of the app `appPid`.
 *
 * @param {Proc[]} table
 * @param {number} appPid
 * @returns {number[]}
 */
export function findWebContentProcesses(table, appPid) {
  return findDescendants(table, appPid, isWebContentProcess);
}

/**
 * Kills a process immediately, on any OS. (`process.kill(pid, "SIGKILL")`
 * maps to TerminateProcess on Windows.)
 *
 * @param {number} pid
 */
export function killNow(pid) {
  process.kill(pid, "SIGKILL");
}
