// Scene data shared by all three concepts. Everything that M1 already does is
// modelled on real behaviour (real repository paths, real cargo output, real
// commands from apps/desktop-tauri/src/App.tsx). Everything belonging to a
// later milestone is marked `mock` and labelled in the UI.

import executorSrc from "../../../crates/exec/src/executor.rs?raw";
import libSrc from "../../../crates/exec/src/lib.rs?raw";
import treeSrc from "../../../crates/exec/src/tree.rs?raw";
import ipcDoc from "../../../docs/protocols/ipc.md?raw";

export interface Node {
  name: string;
  kind: "dir" | "file";
  depth: number;
  open?: boolean;
  ignored?: boolean;
  tag?: "teal" | "amber" | "sky" | "coral" | "lime";
  path: string;
}

const d = (path: string, depth: number, open = false, extra: Partial<Node> = {}): Node => ({
  name: path.split("/").pop()!,
  kind: "dir",
  depth,
  open,
  path,
  ...extra,
});
const f = (path: string, depth: number, extra: Partial<Node> = {}): Node => ({
  name: path.split("/").pop()!,
  kind: "file",
  depth,
  path,
  ...extra,
});

export const TREE: Node[] = [
  d(".github", 0),
  d("agents", 0),
  d("apps", 0, false, { tag: "amber" }),
  d("crates", 0, true, { tag: "teal" }),
  d("crates/axiomd", 1),
  d("crates/desktop-bridge", 1),
  d("crates/exec", 1, true),
  d("crates/exec/src", 2, true),
  f("crates/exec/src/backend.rs", 3),
  f("crates/exec/src/error.rs", 3),
  f("crates/exec/src/executor.rs", 3),
  f("crates/exec/src/lib.rs", 3),
  f("crates/exec/src/tree.rs", 3),
  f("crates/exec/src/tree_windows.rs", 3),
  f("crates/exec/src/utf8.rs", 3),
  d("crates/exec/tests", 2),
  f("crates/exec/Cargo.toml", 2),
  d("crates/protocol", 1),
  d("crates/rpc", 1),
  d("crates/store", 1),
  d("crates/workspace", 1),
  d("docs", 0, false, { tag: "sky" }),
  d("editor", 0),
  d("providers", 0),
  d("schemas", 0),
  d("scripts", 0),
  d("target", 0, false, { ignored: true }),
  f(".gitignore", 0),
  f("AGENTS.md", 0),
  f("Cargo.lock", 0),
  f("Cargo.toml", 0),
  f("deny.toml", 0),
  f("README.md", 0),
];

export const ACTIVE_FILE = "crates/exec/src/executor.rs";

export interface Doc {
  path: string;
  title: string;
  language: string;
  text: string;
  dirty?: boolean;
}

export const DOCS: Doc[] = [
  { path: "crates/exec/src/executor.rs", title: "executor.rs", language: "rust", text: executorSrc },
  { path: "crates/exec/src/tree.rs", title: "tree.rs", language: "rust", text: treeSrc, dirty: true },
  { path: "crates/exec/src/lib.rs", title: "lib.rs", language: "rust", text: libSrc },
  { path: "docs/protocols/ipc.md", title: "ipc.md", language: "markdown", text: ipcDoc },
];

/** Breadcrumb for the active document (path segments, then the symbol at the cursor). */
export const CRUMBS = ["crates", "exec", "src", "executor.rs", "impl Executor", "fn spawn"];

export type Stream = "stdout" | "stderr";
export interface OutLine {
  text: string;
  stream: Stream;
  kind?: "ok" | "warn" | "running" | "summary";
}

// cargo prints progress ("Compiling", "Finished", "Running") on stderr: a
// design that paints all stderr red would paint a healthy build red.
export const OUTPUT: OutLine[] = [
  { text: "   Compiling axiom-protocol v0.1.0 (crates/protocol)", stream: "stderr" },
  { text: "   Compiling axiom-exec v0.1.0 (crates/exec)", stream: "stderr" },
  { text: "warning: unused variable: `grace`", stream: "stderr", kind: "warn" },
  { text: "   --> crates/exec/src/executor.rs:214:13", stream: "stderr", kind: "warn" },
  { text: "    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.81s", stream: "stderr" },
  { text: "     Running unittests src/lib.rs (target/debug/deps/axiom_exec-3f2a91c0d7e4b812)", stream: "stderr" },
  { text: "", stream: "stdout" },
  { text: "running 14 tests", stream: "stdout" },
  { text: "test utf8::tests::splits_multibyte_boundary ... ok", stream: "stdout", kind: "ok" },
  { text: "test utf8::tests::flushes_invalid_tail ... ok", stream: "stdout", kind: "ok" },
  { text: "test executor::tests::label_defaults_to_argv ... ok", stream: "stdout", kind: "ok" },
  { text: "test executor::tests::streams_stdout_and_stderr_in_order ... ok", stream: "stdout", kind: "ok" },
  { text: "test executor::tests::history_is_bounded_per_workspace ... ok", stream: "stdout", kind: "ok" },
  { text: "test executor::tests::cancel_kills_process_tree ... ok", stream: "stdout", kind: "ok" },
  { text: "test executor::tests::exit_status_reports_signal ... ok", stream: "stdout", kind: "ok" },
  { text: "test executor::tests::kill_grace_escalates_to_sigkill has been running for over 60 seconds", stream: "stdout", kind: "running" },
];

export const TASK = {
  label: "cargo test -p axiom-exec",
  elapsed: "00:12.4",
  state: "running" as const,
  history: ["cargo test -p axiom-exec", "cargo clippy --workspace -- -D warnings", "scripts/check.sh rust"],
};

// ---- M3 mock: AI workspace (tool names from docs/protocols/agents.md) ----
export interface AgentStep {
  kind: "user" | "assistant" | "tool" | "approval" | "checkpoint" | "diff";
  text?: string;
  tool?: string;
  target?: string;
  state?: "done" | "running" | "waiting" | "failed";
  meta?: string;
}

export const AGENT_SESSIONS = [
  { id: "s1", title: "Tighten kill-grace escalation", state: "waiting" as const, model: "claude-sonnet", tag: "coral" },
  { id: "s2", title: "Explain ignore_rules matching", state: "idle" as const, model: "local · qwen3-coder", tag: "sky" },
];

export const AGENT_STEPS: AgentStep[] = [
  { kind: "user", text: "The kill-grace test hangs on Linux. Make the executor escalate to SIGKILL after KILL_GRACE and keep the test deterministic." },
  { kind: "tool", tool: "fs.read", target: "crates/exec/src/executor.rs", meta: "lines 180–260", state: "done" },
  { kind: "tool", tool: "workspace.search", target: "KILL_GRACE", meta: "3 matches", state: "done" },
  { kind: "assistant", text: "The grace period is computed but never awaited: `kill()` is sent and the watcher returns before the timer fires. I'll await the timer inside the cancel branch and add a short grace for the test." },
  { kind: "checkpoint", text: "Checkpoint 3", meta: "before patch · 14:02" },
  { kind: "diff", target: "crates/exec/src/executor.rs", meta: "+6 −2" },
  { kind: "approval", tool: "fs.applyPatch", target: "crates/exec/src/executor.rs", meta: "WriteWorkspace · ask" },
  { kind: "tool", tool: "task.run", target: "cargo test -p axiom-exec", meta: "allowed by rule: cargo", state: "running" },
];

export const DIFF = [
  { t: "ctx", n: 212, s: "            _ = cancel.notified() => {" },
  { t: "del", n: 213, s: "                lock(&tree).kill();" },
  { t: "del", n: 214, s: "                let grace = KILL_GRACE;" },
  { t: "add", n: 213, s: "                lock(&tree).terminate();" },
  { t: "add", n: 214, s: "                if timeout(KILL_GRACE, done.changed()).await.is_err() {" },
  { t: "add", n: 215, s: "                    lock(&tree).kill();" },
  { t: "add", n: 216, s: "                }" },
  { t: "ctx", n: 217, s: "            }" },
];

// ---- M2 mock: Environment Manager (provider ids from providers/) ----
export const ENVIRONMENTS = [
  { id: "rust", name: "Rust", tools: "cargo · rustc · rustup", detected: "1.99.1", source: "rustup (~/.cargo)", state: "ok" as const, required: "≥ 1.85 (rust-toolchain.toml)" },
  { id: "node", name: "Node.js", tools: "node · pnpm", detected: "22.23.1", source: "system (/usr/bin)", state: "ok" as const, required: "22.x (package.json engines)" },
  { id: "cmake", name: "CMake + Ninja", tools: "cmake · ninja", detected: null, source: "—", state: "installing" as const, required: "≥ 3.28 (experiments/e2)", progress: 0.62, step: "Verifying SHA-256 · cmake-4.1.2-linux-x86_64.tar.gz" },
  { id: "python", name: "Python", tools: "uv · python", detected: null, source: "—", state: "missing" as const, required: "3.12 (bench/)" },
];

export const DIAGNOSTICS = [
  { sev: "warning" as const, file: "crates/exec/src/executor.rs", line: 214, col: 13, msg: "unused variable: `grace`", code: "unused_variables" },
  { sev: "error" as const, file: "crates/exec/tests/kill.rs", line: 41, col: 9, msg: "mismatched types: expected `Duration`, found `u64`", code: "E0308" },
];

// ---- command palette (real M1 commands from App.tsx; future ones mocked) ----
export const COMMANDS = [
  { group: "Run", label: "Rerun: cargo test -p axiom-exec", key: "F5" },
  { group: "Run", label: "Run Command…", key: "Ctrl Shift R" },
  { group: "Run", label: "Stop", key: "Shift F5" },
  { group: "File", label: "Save", key: "Ctrl S" },
  { group: "File", label: "Save All" },
  { group: "File", label: "Open Folder…", key: "Ctrl O" },
  { group: "View", label: "Toggle Side Bar", key: "Ctrl B" },
  { group: "View", label: "Toggle Panel", key: "Ctrl J" },
  { group: "View", label: "Switch to Light Theme" },
  { group: "Backend", label: "Restart Backend" },
  { group: "Recent", label: "axiom — ~/src/axiom" },
];

export const FILE_RESULTS = [
  { path: "crates/exec/src/executor.rs", match: [11, 13] },
  { path: "crates/exec/tests/executor.rs", match: [11, 13] },
  { path: "crates/exec/src/error.rs", match: [11, 13] },
  { path: "crates/exec/Cargo.toml", match: [0, 0] },
];
