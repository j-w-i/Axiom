#!/usr/bin/env node
// The M2 exit scenario (M2-QA-001; docs/roadmap/milestones.md §M2) on a machine without Rust,
// through the UI and the protocol only:
//   open a Cargo project → Axiom reports Rust missing → the install plan → install a managed
//   toolchain → `cargo build` runs inside it → the compile error appears in Problems → Enter
//   opens the file at the line → Remove deletes that toolchain directory and nothing else.
//
// "Without Rust": the app (and so the daemon and every process it starts) gets a PATH with every
// directory that holds cargo, rustc or rustup removed, and a fresh empty HOME; the suite asserts
// both stay that way. The project selects its toolchain in `.axiom/project.toml`, as a real
// project would. Opening the folder is the one step not done by input: the native folder dialog
// cannot be driven, so the folder is opened through the store's own request (as smoke does).
// Needs the internet (static.rust-lang.org). Usage: scripts/e2e.sh env.mjs
// Env: AXIOM_E2E_RUST_VERSION (default 1.84.0).

import { existsSync, mkdirSync, mkdtempSync, readdirSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
const VERSION = process.env.AXIOM_E2E_RUST_VERSION ?? "1.84.0";
mkdirSync(SHOTS, { recursive: true });
const onLog = appLogger("env");

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const elapsed = () => `${((Date.now() - t0) / 1000).toFixed(1)} s`;
const timings = {};

// ---- a machine without Rust ------------------------------------------------------------
const rustTools = ["cargo", "rustc", "rustup"].map((t) => t + EXE);
const fullPath = (process.env.PATH ?? "").split(delimiter).filter(Boolean);
// The external WebDriver (tauri-driver) is a cargo-installed program, often next to cargo: it
// alone is linked into a private directory, so the app's PATH still has no Rust.
const driverDir = mkdtempSync(join(tmpdir(), "axiom-env-driver-"));
const tauriDriver = fullPath.map((d) => join(d, "tauri-driver" + EXE)).find((p) => existsSync(p));
if (tauriDriver) symlinkSync(tauriDriver, join(driverDir, "tauri-driver" + EXE));
const cleanPath = [driverDir, ...fullPath.filter((d) => !rustTools.some((t) => existsSync(join(d, t))))].join(delimiter);
const home = mkdtempSync(join(tmpdir(), "axiom-env-home-"));
const dataDir = mkdtempSync(join(tmpdir(), "axiom-env-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-env-ws-"));
writeFileSync(join(ws, "Cargo.toml"), '[package]\nname = "exit"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n');
mkdirSync(join(ws, "src"));
writeFileSync(join(ws, "src", "main.rs"), 'fn main() {\n    let x: i32 = "text";\n    println!("{x}");\n}\n');
mkdirSync(join(ws, ".axiom"));
writeFileSync(join(ws, ".axiom", "project.toml"), `[environment]\nrust = "${VERSION}"\n`);

/** Every path under `dir`, relative, `/`-separated, sorted. */
function snapshot(dir) {
  const out = [];
  const walk = (d, rel) => {
    for (const name of readdirSync(d)) {
      const p = join(d, name);
      const r = rel ? `${rel}/${name}` : name;
      out.push(r);
      const st = statSync(p, { throwIfNoEntry: false });
      if (st?.isDirectory()) walk(p, r);
    }
  };
  if (existsSync(dir)) walk(dir, "");
  return out.sort();
}

const appExpr = "window.__axiom";
const row = `.env-table [data-provider="rust"]`;
let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", dataDir], { PATH: cleanPath, HOME: home, USERPROFILE: home }, onLog);
  await wd.waitFor(`return ${appExpr}?.client.state.state === 'ready'`, [], 30000, "backend ready");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="src"]')`, [], 15000, "workspace open");

  // 1. Rust is reported missing, with the project's selection.
  await wd.click(await wd.find('.rail [data-destination="environments"]'));
  await wd.waitFor(`return document.querySelector(${JSON.stringify(row)})?.dataset.state === 'missing'`, [], 30000, "rust missing");
  const missing = await wd.exec(
    `const r = document.querySelector(${JSON.stringify(row)}); return [...r.querySelectorAll('td')].map(td => td.textContent.trim())`,
  );
  check(
    "1 a Cargo project on a machine without Rust: Rust is missing, selected by .axiom/project.toml, with Plan install…",
    missing[0] === `${VERSION} (.axiom/project.toml)` && missing[1] === "not found" && missing[3].includes("Plan install"),
    JSON.stringify(missing),
  );
  writeFileSync(join(SHOTS, "env-1-missing.png"), await wd.screenshot());

  // 2. The install plan.
  await wd.click(await wd.find(`${row} .btn--primary`));
  await wd.waitFor(`return !!document.querySelector('[data-testid=install-plan] .plan-step')`, [], 60000, "plan dialog");
  const plan = await wd.exec(
    `return [...document.querySelectorAll('[data-testid=install-plan] .plan__steps > li')].map(li => li.dataset.kind + ' ' + li.querySelector('.plan-step__name').textContent.trim() + ' | ' + [...li.querySelectorAll('code')].map(c => c.textContent).join(' '))`,
  );
  check(
    "2 the plan shows every step: rustup-init download with its checksum file, place, set up rustup, install the toolchain, verify cargo and rustc",
    plan.length === 6 && plan[0].startsWith("download Download rustup-init") && plan[0].includes(".sha256") && plan[1].startsWith("extract") &&
      plan[2].startsWith("run Run rustup-init") && plan[3].startsWith("run Run rustup") && plan[3].includes(`toolchain install ${VERSION}`) &&
      plan[4] === "verify Check that cargo runs | " && plan[5] === "verify Check that rustc runs | ",
    JSON.stringify(plan.map((p) => p.slice(0, 90))),
  );
  writeFileSync(join(SHOTS, "env-2-plan.png"), await wd.screenshot());

  // 3. Install (the approval), through to Ready.
  let started = Date.now();
  await wd.click(await wd.find("[data-testid=install-plan] [data-action=install]"));
  await wd.waitFor(`return document.querySelector(${JSON.stringify(row)})?.dataset.state === 'installing'`, [], 30000, "installing");
  writeFileSync(join(SHOTS, "env-3-installing.png"), await wd.screenshot());
  const ready = await wd
    .waitFor(
      `const r = document.querySelector(${JSON.stringify(row)}); return r?.dataset.state === 'ready' ? [...r.querySelectorAll('td')].map(td => td.textContent.trim()) : (r?.dataset.state === 'error' ? 'error: ' + r.querySelector('.env-state__error')?.textContent : null)`,
      [],
      900000,
      "rust ready",
    )
    .catch((e) => `timeout: ${e.message}`);
  timings.install = (Date.now() - started) / 1000;
  const root = join(dataDir, "toolchains", "rust", VERSION);
  check(
    "3 Install runs the approved plan; the row is Ready with the managed toolchain",
    Array.isArray(ready) && ready[1] === VERSION && ready[2].startsWith("managed") && existsSync(root),
    `${JSON.stringify(ready)} in ${timings.install.toFixed(1)} s`,
  );
  const afterInstall = snapshot(dataDir).filter((p) => !p.startsWith("cache"));

  // 4. cargo build inside it, from the palette; the error in Problems; Enter opens it.
  await wd.click(await wd.find('.rail [data-destination="files"]'));
  // Focus in the explorer (no document opening in the background to take focus back).
  await wd.click(await wd.find('[data-path="src"]'));
  await wd.waitFor(`return !!document.activeElement?.closest('[data-region=files]')`, [], 5000, "focus in the explorer");
  await wd.chord(MOD, "k");
  await wd.waitFor(`return !!document.querySelector('[data-testid=palette] [role=option]') && !!document.activeElement?.closest('[data-testid=palette]')`, [], 3000, "palette focused");
  await wd.keys([..."Build: cargo build", ""]);
  started = Date.now();
  const outcome = await wd
    .waitFor(`const s = ${appExpr}.tasks.getSnapshot(); return !s.running && s.lastOutcome ? s.lastOutcome : null`, [], 300000, "build finished")
    .catch(() => null);
  timings.build = (Date.now() - started) / 1000;
  const usedManaged = await wd.exec(`return ${appExpr}.tasks.getSnapshot().rows.map(r => r.text).join('\\n')`);
  check(
    "4 Build: cargo build runs inside the managed toolchain (no Rust on PATH) and fails on the type error",
    outcome === "failed" && /error\[E0308\]|mismatched types/.test(usedManaged),
    `outcome ${outcome} in ${timings.build.toFixed(1)} s`,
  );
  await wd.click(await wd.find("[data-view=problems]"));
  await wd.waitFor(`return !!document.querySelector('[data-testid=problems] .problem[data-severity=error]')`, [], 10000, "problem listed");
  const name = await wd.exec(`return document.querySelector('[data-testid=problems] .problem[data-severity=error]').getAttribute('aria-label')`);
  writeFileSync(join(SHOTS, "env-4-problems.png"), await wd.screenshot());
  await wd.exec(`document.querySelector('[data-testid=problems] .problem[data-severity=error]').focus()`);
  await wd.keys([""]);
  const at = await wd
    .waitFor(
      `const t = document.querySelector('[role=tab][aria-selected=true][data-path]')?.dataset.path; const c = ${appExpr}.documents.getSnapshot().cursorText; return t === 'src/main.rs' && c.startsWith('Ln 2, Col 18') ? c : null`,
      [],
      10000,
      "file at the error",
    )
    .catch(() => null);
  check("5 the compile error is in Problems; Enter opens src/main.rs at line 2, column 18", /mismatched types \(E0308\) at src\/main\.rs:2:18$/.test(name) && at !== null, `${name} → ${at}`);

  // 5. Remove: exactly that directory goes.
  await wd.click(await wd.find('.rail [data-destination="environments"]'));
  await wd.waitFor(`return document.querySelector(${JSON.stringify(row)})?.dataset.state === 'ready'`, [], 10000, "row ready");
  await wd.click(await wd.find(`${row} .env-actions .icon-btn`));
  await wd.waitFor(`return !!document.querySelector('[data-testid=remove-toolchain]')`, [], 5000, "remove dialog");
  const confirm = await wd.exec(`return document.querySelector('[data-testid=remove-toolchain] .dialog__body')?.textContent`);
  await wd.click(await wd.find("[data-testid=remove-toolchain] [data-action=remove]"));
  await wd.waitFor(`return document.querySelector(${JSON.stringify(row)})?.dataset.state === 'missing'`, [], 120000, "rust missing again");
  const afterRemove = snapshot(dataDir).filter((p) => !p.startsWith("cache"));
  const prefix = `toolchains/rust/${VERSION}`;
  const expected = afterInstall.filter((p) => p !== prefix && !p.startsWith(prefix + "/"));
  const removedOnly = JSON.stringify(afterRemove) === JSON.stringify(expected);
  const homeAfter = snapshot(home).filter((p) => !/^(\.cache|\.local|\.config)(\/|$)/.test(p));
  check(
    "6 Remove names the directory and deletes it and nothing else (data-dir snapshot); HOME has no .cargo or .rustup",
    removedOnly && !existsSync(root) && confirm?.includes(VERSION) && !homeAfter.some((p) => p.startsWith(".cargo") || p.startsWith(".rustup")),
    `snapshot diff ${removedOnly ? "empty" : JSON.stringify({ extra: afterRemove.filter((p) => !expected.includes(p)), missing: expected.filter((p) => !afterRemove.includes(p)) })}; ` +
      `${afterInstall.length - expected.length} paths removed; HOME: ${JSON.stringify(homeAfter.slice(0, 5))}`,
  );
  writeFileSync(join(SHOTS, "env-5-removed.png"), await wd.screenshot());
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
  if (wd) writeFileSync(join(SHOTS, "env-zz-failure.png"), await wd.screenshot().catch(() => Buffer.alloc(0)));
} finally {
  await wd?.quit().catch(() => {});
  for (const d of [dataDir, ws, home, driverDir]) await removeDir(d).catch(() => {});
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} env checks passed in ${elapsed()}; timings ${JSON.stringify(timings)}`);
process.exit(failed.length ? 1 : 0);
