// Finding (and killing) a webview's content process, portably.
//
// The end-to-end crash test (T-08) must kill the process that renders the page:
// WebKitWebProcess on Linux, msedgewebview2.exe on Windows, a WebContent
// process on macOS. This module keeps the *logic* pure and testable
// (`findWebContentProcesses`) and isolates the one platform call
// (`readProcessTable`), so `tests/processes.test.ts` covers the decisions on
// every OS without needing one.

import { execFileSync } from "node:child_process";

/**
 * @typedef {{ pid: number, ppid: number, comm: string }} Proc
 */

/** Names of webview content processes. `ps comm` truncates to 15 characters. */
export const CONTENT_PROCESS = /WebKitWebProc|msedgewebview2|WebContent|WebKit\.WebContent/i;

/**
 * Parses `ps -e -o pid=,ppid=,comm=` output (Linux and macOS).
 * Lines that do not parse are skipped rather than throwing: `ps` output is not
 * a contract, and a missing row must never silently become process 0.
 *
 * @param {string} text
 * @returns {Proc[]}
 */
export function parsePsTable(text) {
  const out = [];
  for (const line of text.split("\n")) {
    const m = /^\s*(\d+)\s+(\d+)\s+(.+?)\s*$/.exec(line);
    if (!m) continue;
    out.push({ pid: Number(m[1]), ppid: Number(m[2]), comm: m[3] });
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
        "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name | ConvertTo-Json -Compress",
      ],
      { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
    );
    return parseWindowsTable(json);
  }
  const ps = execFileSync("ps", ["-e", "-o", "pid=,ppid=,comm="], { encoding: "utf8" });
  return parsePsTable(ps);
}

/**
 * Processes whose name matches `name` **and** that are descendants of
 * `appPid`. Descendants only — another application's process must never be
 * touched, which is also why there is no "fall back to any match".
 *
 * @param {Proc[]} table
 * @param {number} appPid
 * @param {RegExp} name
 * @returns {number[]}
 */
export function findDescendantsByName(table, appPid, name) {
  const byPid = new Map(table.map((p) => [p.pid, p]));
  const isDescendant = (pid) => {
    let p = byPid.get(pid);
    for (let i = 0; i < 16 && p && p.ppid > 1; i++) {
      if (p.ppid === appPid) return true;
      p = byPid.get(p.ppid);
    }
    return false;
  };
  return table.filter((p) => name.test(p.comm) && isDescendant(p.pid)).map((p) => p.pid);
}

/**
 * The content processes of the app `appPid`: matching names **and** a
 * descendant of it. Descendants only — another application's webview must
 * never be touched, which is also why there is no "fall back to any match".
 *
 * @param {Proc[]} table
 * @param {number} appPid
 * @returns {number[]}
 */
export function findWebContentProcesses(table, appPid) {
  return findDescendantsByName(table, appPid, CONTENT_PROCESS);
}
