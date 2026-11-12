#!/usr/bin/env python3
"""Qt vs Tauri benchmark harness (docs/migration/qt-to-tauri/performance.md).

Runs both shells with identical inputs on a PRIVATE Xvfb (never the user's
desktop: scripts/headless.sh removes Wayland and forces X11), against the same
axiomd binary, wrapped in bench/daemon_tap.py so that one protocol-level clock
measures both. Real X input is injected with XTEST; "pixels changed" is
observed with the X DAMAGE extension on the app's top-level window. This
treats both apps as black boxes: no test hooks, no app-side instrumentation.

Usage:  bench/run.sh [--runs N] [--only qt|tauri] [--scenario all|startup|...]
Output: JSON on stdout and bench/results/<host>-<date>.json

Scenarios
  startup      cold (first launch after dropping our files from page cache is
               not possible without root, so "cold" = first launch of a fresh
               data dir after a 3 s idle) and warm (repeat) launch →
               daemon `initialize` answered, first `fs/list` answered, first
               DAMAGE after that answer (explorer painted).
  memory       PSS of the whole process tree (shell + webview/renderer
               helpers; daemon reported separately) at idle with a workspace
               open, and after opening a file in Monaco.
  cpu          CPU seconds of the process tree over 10 s idle with a
               workspace and a document open.
  typing       keystroke → DAMAGE latency in Monaco, 200 keystrokes, real
               XTEST key events, p50/p95/p99.
  explorer     time from fs/list answer for a 5 000-entry directory to the
               window repaint (DAMAGE).
  output       a task printing 50 000 lines: task/exited → first DAMAGE after
               it ("output caught up") and CPU used meanwhile.
"""
import argparse
import json
import os
import platform
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import psutil
from Xlib import X, XK, display
from Xlib.ext import damage, xtest

HERE = Path(__file__).resolve().parent
APP_DIR = HERE.parent
REPO = APP_DIR.parent.parent
TAP = HERE / "daemon_tap.py"
DAEMON = Path(os.environ.get("AXIOMD_UNDER_TEST", REPO / "target/release/axiomd"))
QT_APP = Path(os.environ.get("AXIOM_QT_APP", REPO / "build/desktop/axiom"))
QT_ASSETS = Path(os.environ.get("AXIOM_QT_EDITOR_ASSETS", REPO / "build/desktop/editor"))
TAURI_APP = Path(os.environ.get("AXIOM_TAURI_APP", REPO / "target/release/axiom-tauri"))


# ---------------------------------------------------------------------------
# X helpers
# ---------------------------------------------------------------------------

class XSession:
    def __init__(self):
        self.d = display.Display()
        if not self.d.has_extension("DAMAGE") or not self.d.has_extension("XTEST"):
            raise SystemExit("X server lacks DAMAGE/XTEST")
        self.root = self.d.screen().root
        self.dmg = None
        # Windows come and go during startup; X errors about them are expected.
        self.d.set_error_handler(lambda *a, **k: None)

    def windows_of(self, pids):
        """Top-level mapped windows whose _NET_WM_PID is in pids (or any client of them)."""
        atom = self.d.intern_atom("_NET_WM_PID")
        out = []

        def walk(w):
            try:
                attrs = w.get_attributes()
                geom = w.get_geometry()
            except Exception:
                return
            try:
                p = w.get_full_property(atom, X.AnyPropertyType)
                if p and p.value and int(p.value[0]) in pids and attrs.map_state == X.IsViewable and geom.width > 300:
                    out.append(w)
            except Exception:
                pass
            try:
                for c in w.query_tree().children:
                    walk(c)
            except Exception:
                pass

        walk(self.root)
        return out

    def wait_window(self, pids_fn, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            ws = self.windows_of(set(pids_fn()))
            if ws:
                return max(ws, key=lambda w: w.get_geometry().width * w.get_geometry().height)
            time.sleep(0.02)
        raise TimeoutError("no window")

    # ---- pixels ------------------------------------------------------------
    # Repaint is observed as *pixels changing on the X server*, sampled with
    # XGetImage at ~1 kHz over a region. DAMAGE is not usable here: Xvfb does
    # not report child-window drawing on the root, and both toolkits draw into
    # sub-windows. Sampling is toolkit-agnostic, so both shells are measured
    # identically (resolution ≈ the sampling period, recorded per result).

    def grab(self, region):
        x0, y0, w, h = region
        return hash(self.root.get_image(x0, y0, w, h, X.ZPixmap, 0xFFFFFFFF).data)

    def wait_change(self, region, before, timeout=5.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.grab(region) != before:
                return time.monotonic()
        return None

    def wait_stable(self, region, quiet=0.5, timeout=20):
        last_h, last_t = self.grab(region), time.monotonic()
        deadline = last_t + timeout
        while time.monotonic() < deadline:
            time.sleep(0.01)
            h = self.grab(region)
            if h != last_h:
                last_h, last_t = h, time.monotonic()
            elif time.monotonic() - last_t >= quiet:
                return last_t
        return None

    def record(self, region, until):
        """[(t, hash)] samples of `region` until `until()` is true or 30 s."""
        out = []
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            out.append((time.monotonic(), self.grab(region)))
            if until():
                break
        return out

    def focus(self, win):
        self.d.set_input_focus(win, X.RevertToParent, X.CurrentTime)
        self.d.sync()

    def origin(self, win):
        abs_ = win.translate_coords(self.root, 0, 0)
        return -abs_.x, -abs_.y

    def region(self, win, fx0, fy0, fx1, fy1):
        """Window-relative fractional rectangle → absolute (x, y, w, h)."""
        g = win.get_geometry()
        ox, oy = self.origin(win)
        return (ox + int(g.width * fx0), oy + int(g.height * fy0), int(g.width * (fx1 - fx0)), int(g.height * (fy1 - fy0)))

    def click(self, win, x, y):
        ox, oy = self.origin(win)
        ax, ay = ox + x, oy + y
        xtest.fake_input(self.d, X.MotionNotify, x=ax, y=ay)
        xtest.fake_input(self.d, X.ButtonPress, 1)
        xtest.fake_input(self.d, X.ButtonRelease, 1)
        self.d.sync()

    def key(self, keysym_name, mods=()):
        codes = [self.d.keysym_to_keycode(XK.string_to_keysym(m)) for m in mods]
        kc = self.d.keysym_to_keycode(XK.string_to_keysym(keysym_name))
        for c in codes:
            xtest.fake_input(self.d, X.KeyPress, c)
        xtest.fake_input(self.d, X.KeyPress, kc)
        xtest.fake_input(self.d, X.KeyRelease, kc)
        for c in reversed(codes):
            xtest.fake_input(self.d, X.KeyRelease, c)
        self.d.sync()


# ---------------------------------------------------------------------------
# process helpers
# ---------------------------------------------------------------------------

def tree(pid):
    try:
        p = psutil.Process(pid)
        return [p, *p.children(recursive=True)]
    except psutil.NoSuchProcess:
        return []


def pss_mib(procs):
    total = 0
    for p in procs:
        try:
            total += p.memory_full_info().pss
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return total / (1024 * 1024)


def idle_cpu_percent(procs, windows=3, seconds=10.0):
    """Idle CPU of the tree, sampled in several windows.

    Other work on the host can only *add* CPU to a process's numbers, never
    remove it, so the minimum window is the best estimate of the application's
    own idle cost and the spread says how noisy the host was. All three are
    returned; the median is what the single-window field has always meant.
    """
    samples = []
    for _ in range(windows):
        c0 = cpu_seconds(procs)
        time.sleep(seconds)
        samples.append((cpu_seconds(procs) - c0) / seconds * 100)
    samples.sort()
    return {
        "pct": round(samples[0], 2),
        "min": round(samples[0], 2),
        "median": round(samples[len(samples) // 2], 2),
        "max": round(samples[-1], 2),
        "windows": windows,
    }


def cpu_seconds(procs):
    total = 0.0
    for p in procs:
        try:
            t = p.cpu_times()
            total += t.user + t.system + getattr(t, "children_user", 0) * 0 + getattr(t, "children_system", 0) * 0
        except psutil.NoSuchProcess:
            pass
    return total


def read_tap(path):
    out = []
    if path.exists():
        for line in path.read_text().splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                pass
    return out


def tap_time(events, event, method=None, nth=0):
    hits = [e for e in events if e["event"] == event and (method is None or e.get("method") == method)]
    return hits[nth]["t"] if len(hits) > nth else None


# ---------------------------------------------------------------------------
# launching
# ---------------------------------------------------------------------------

class App:
    def __init__(self, kind, data_dir, tap_log):
        self.kind = kind
        self.data_dir = data_dir
        self.tap_log = tap_log
        wrapper = Path(data_dir) / "axiomd-tap"
        wrapper.write_text(f"#!/bin/sh\nexec {sys.executable} {TAP} \"$@\"\n")
        wrapper.chmod(0o755)
        env = dict(os.environ)
        env.update(AXIOM_TAP_LOG=str(tap_log), AXIOM_TAP_REAL=str(DAEMON))
        if kind == "qt":
            env.update(QT_QPA_PLATFORM="xcb", QTWEBENGINE_CHROMIUM_FLAGS="--no-sandbox")
            cmd = [str(QT_APP), "--daemon", str(wrapper), "--data-dir", data_dir, "--editor-assets", str(QT_ASSETS)]
            env["XDG_CONFIG_HOME"] = str(Path(data_dir) / "config")
        else:
            cmd = [str(TAURI_APP), "--daemon", str(wrapper), "--data-dir", data_dir]
            env["XDG_CONFIG_HOME"] = str(Path(data_dir) / "config")
            env["XDG_DATA_HOME"] = str(Path(data_dir) / "share")
            env["XDG_CACHE_HOME"] = str(Path(data_dir) / "cache")
        self.t_launch = time.monotonic()
        self.proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)

    def pids(self):
        return [p.pid for p in tree(self.proc.pid)]

    def procs_split(self):
        """(shell+webview procs, daemon procs) — the daemon is identical in both and reported apart."""
        shell, daemon = [], []
        for p in tree(self.proc.pid):
            try:
                cmd = " ".join(p.cmdline())
            except psutil.NoSuchProcess:
                continue
            if "daemon_tap.py" in cmd or cmd.startswith(str(DAEMON)):
                daemon.append(p)
            else:
                shell.append(p)
        return shell, daemon

    def stop(self):
        try:
            os.killpg(self.proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            return
        try:
            self.proc.wait(5)
        except subprocess.TimeoutExpired:
            os.killpg(self.proc.pid, signal.SIGKILL)
            self.proc.wait(5)


def make_workspace(root, files=40, big_dir=0):
    root = Path(root)
    (root / "src").mkdir(parents=True)
    for i in range(files):
        (root / "src" / f"module_{i:03}.rs").write_text(f"pub fn f{i}() -> u32 {{ {i} }}\n" * 20)
    (root / "README.md").write_text("# bench\n")
    main = "\n".join(f"fn line_{i}() {{ println!(\"{i}\"); }}" for i in range(400))
    (root / "main.rs").write_text(main + "\n")
    if big_dir:
        b = root / "big"
        b.mkdir()
        for i in range(big_dir):
            (b / f"f{i:05}.txt").write_text("x\n")
    return root


def seed_state(data_dir, workspace, open_files=(), expanded=()):
    """Pre-seeds the store through the protocol (reopen-last + tabs), using the daemon directly."""
    proc = subprocess.Popen([str(DAEMON), "--data-dir", data_dir], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    msgs = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "0.1", "client": {"name": "bench", "version": "0"}}},
        {"jsonrpc": "2.0", "id": 2, "method": "workspace/open", "params": {"path": str(workspace)}},
    ]
    for m in msgs:
        proc.stdin.write((json.dumps(m) + "\n").encode())
        proc.stdin.flush()
        json.loads(proc.stdout.readline())
    proc.stdin.write((json.dumps({"jsonrpc": "2.0", "id": 3, "method": "workspace/setState", "params": {"workspaceId": "w1", "values": {
        "editor.openPaths": list(open_files), "editor.activePath": open_files[0] if open_files else None, "explorer.expanded": list(expanded)}}}) + "\n").encode())
    proc.stdin.flush()
    json.loads(proc.stdout.readline())
    proc.stdin.close()
    proc.wait(5)


# ---------------------------------------------------------------------------
# scenarios
# ---------------------------------------------------------------------------

# Layout facts per shell (fractions of the window), from calibration
# screenshots of each app at its default size (Qt 1100x720, Tauri 1180x760).
LAYOUT = {
    "qt": {"command_field": (0.57, 0.676), "output": (0.25, 0.72, 0.97, 0.94)},
    "tauri": {"command_field": (0.585, 0.704), "output": (0.23, 0.74, 0.97, 0.95)},
}
# Regions (fractions of the window): the explorer column and the editor body.
EXPLORER = (0.0, 0.08, 0.20, 0.60)
EDITOR = (0.25, 0.10, 0.95, 0.60)


def wait_tap(tap, method, nth=0, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        t = tap_time(read_tap(tap), "response", method, nth)
        if t is not None:
            return t
        time.sleep(0.002)
    return None


def first_change_after(samples, t_after):
    """First sample at/after t_after whose hash differs from the one before t_after."""
    before = None
    for t, h in samples:
        if t < t_after:
            before = h
            continue
        if before is not None and h != before:
            return t
        if before is None:
            before = h
    return None


def scenario_startup(x, kind, runs):
    res = {"cold": [], "warm": []}
    base = Path(tempfile.mkdtemp(prefix=f"bench-start-{kind}-"))
    ws = make_workspace(base / "ws")
    for i in range(runs):
        data = base / f"data{i}"
        data.mkdir()
        seed_state(str(data), ws)  # reopen-last: the workspace opens at startup with no input
        for mode in ("cold", "warm"):
            if mode == "cold":
                time.sleep(3)
            tap = data / f"tap-{mode}.jsonl"
            app = App(kind, str(data), tap)
            try:
                win = x.wait_window(app.pids, 30)
                t_window = time.monotonic()
                region = x.region(win, *EXPLORER)
                done = {"t": None}
                samples = x.record(region, lambda: tap_time(read_tap(tap), "response", "fs/list") is not None and time.monotonic() - (tap_time(read_tap(tap), "response", "fs/list") or 0) > 3)
                ev = read_tap(tap)
                t_list = tap_time(ev, "response", "fs/list")
                t_paint = first_change_after(samples, t_list) if t_list else None
                res[mode].append({
                    "window_mapped_ms": round((t_window - app.t_launch) * 1000, 1),
                    "connected_ms": round((tap_time(ev, "response", "initialize") - app.t_launch) * 1000, 1) if tap_time(ev, "response", "initialize") else None,
                    "first_list_answer_ms": round((t_list - app.t_launch) * 1000, 1) if t_list else None,
                    "explorer_painted_ms": round((t_paint - app.t_launch) * 1000, 1) if t_paint else None,
                    "sample_period_ms": round((samples[-1][0] - samples[0][0]) / max(1, len(samples) - 1) * 1000, 2),
                })
            finally:
                app.stop()
    shutil.rmtree(base, ignore_errors=True)
    return res


def scenario_memory_cpu(x, kind):
    base = Path(tempfile.mkdtemp(prefix=f"bench-mem-{kind}-"))
    ws = make_workspace(base / "ws")
    data = base / "data"
    data.mkdir()
    seed_state(str(data), ws)
    tap = data / "tap.jsonl"
    app = App(kind, str(data), tap)
    out = {}
    try:
        win = x.wait_window(app.pids, 30)
        wait_tap(tap, "fs/list")
        x.wait_stable(x.region(win, 0, 0, 1, 1), 1.0, 20)
        time.sleep(3)
        shell, daemon = app.procs_split()
        out["idle_workspace_open_pss_mib"] = round(pss_mib(shell), 1)
        out["daemon_pss_mib"] = round(pss_mib(daemon), 1)
        out["process_count"] = len(shell)
        idle = idle_cpu_percent(shell)
        out["idle_cpu_pct_10s"] = idle["median"]
        out["idle_cpu_pct_min"] = idle["min"]
        out["idle_cpu_pct_max"] = idle["max"]
    finally:
        app.stop()
    # Second launch with a document restored: Monaco loaded and showing a file.
    data2 = base / "data2"
    data2.mkdir()
    seed_state(str(data2), ws, open_files=["main.rs"])
    tap2 = data2 / "tap.jsonl"
    app = App(kind, str(data2), tap2)
    try:
        win = x.wait_window(app.pids, 30)
        t_read = wait_tap(tap2, "fs/readFile", 0, 40)
        x.wait_stable(x.region(win, *EDITOR), 1.5, 30)
        time.sleep(3)
        shell, _ = app.procs_split()
        out["monaco_document_open_pss_mib"] = round(pss_mib(shell), 1)
        out["process_count_with_editor"] = len(shell)
        idle = idle_cpu_percent(shell)
        out["idle_cpu_pct_10s_with_editor"] = idle["median"]
        out["idle_cpu_pct_min_with_editor"] = idle["min"]
        out["idle_cpu_pct_max_with_editor"] = idle["max"]
        out["typing"] = typing_latency(x, win, app)
        shell, _ = app.procs_split()
        out["after_typing_pss_mib"] = round(pss_mib(shell), 1)
    finally:
        app.stop()
    shutil.rmtree(base, ignore_errors=True)
    return out


def typing_latency(x, win, app, n=200):
    g = win.get_geometry()
    # Click into the editor body, go to the end of a line in view.
    x.click(win, int(g.width * 0.55), int(g.height * 0.20))
    time.sleep(0.5)
    x.key("End")
    region = x.region(win, *EDITOR)
    x.wait_stable(region, 0.3, 5)
    shell, _ = app.procs_split()
    c0 = cpu_seconds(shell)
    t_start = time.monotonic()
    lat = []
    letters = "abcdefghijklmnopqrstuvwxyz"
    for i in range(n):
        before = x.grab(region)
        t = time.monotonic()
        x.key(letters[i % 26])
        td = x.wait_change(region, before, 1.0)
        if td is not None:
            lat.append((td - t) * 1000)
        time.sleep(0.03)  # ~25-30 keys/s including the wait: faster than most typists
    cpu = cpu_seconds(shell) - c0
    elapsed = time.monotonic() - t_start
    if not lat:
        return {"samples": 0, "of": n}
    lat.sort()
    q = lambda p: round(lat[min(len(lat) - 1, int(p * len(lat)))], 2)
    return {"samples": len(lat), "of": n, "p50_ms": q(0.5), "p95_ms": q(0.95), "p99_ms": q(0.99),
            "max_ms": round(lat[-1], 2), "mean_ms": round(statistics.mean(lat), 2),
            "cpu_pct_while_typing": round(cpu / elapsed * 100, 1)}


def scenario_explorer(x, kind):
    base = Path(tempfile.mkdtemp(prefix=f"bench-exp-{kind}-"))
    ws = make_workspace(base / "ws", big_dir=5000)
    data = base / "data"
    data.mkdir()
    seed_state(str(data), ws, expanded=["big"])  # restoring expands big/ → one 5 000-entry fs/list
    tap = data / "tap.jsonl"
    app = App(kind, str(data), tap)
    try:
        win = x.wait_window(app.pids, 30)
        region = x.region(win, *EXPLORER)
        samples = x.record(region, lambda: tap_time(read_tap(tap), "response", "fs/list", 1) is not None
                           and time.monotonic() - tap_time(read_tap(tap), "response", "fs/list", 1) > 3)
        ev = read_tap(tap)
        lists = [e for e in ev if e["event"] == "response" and e.get("method") == "fs/list"]
        big = max(lists, key=lambda e: e["bytes"]) if lists else None
        t_big = big["t"] if big else None
        t_paint = first_change_after(samples, t_big) if t_big else None
        # last change after the big answer = settled
        last = None
        prev = None
        for t, h in samples:
            if t_big and t >= t_big and prev is not None and h != prev:
                last = t
            prev = h
        shell, _ = app.procs_split()
        # Scroll the 5 000 rows with the mouse wheel over the explorer (no click:
        # clicking would toggle or open entries). Wheel tick → pixels change.
        g = win.get_geometry()
        ox, oy = x.origin(win)
        xtest.fake_input(x.d, X.MotionNotify, x=ox + int(g.width * 0.08), y=oy + int(g.height * 0.35))
        x.d.sync()
        time.sleep(0.2)
        x.wait_stable(region, 0.3, 5)
        c0 = cpu_seconds(shell)
        frame = []
        for _ in range(60):
            before = x.grab(region)
            t = time.monotonic()
            xtest.fake_input(x.d, X.ButtonPress, 5)
            xtest.fake_input(x.d, X.ButtonRelease, 5)
            x.d.sync()
            td = x.wait_change(region, before, 1.0)
            if td:
                frame.append((td - t) * 1000)
            time.sleep(0.05)
        frame.sort()
        return {
            "entries": 5000,
            "list_response_bytes": big["bytes"] if big else None,
            "list_answer_to_first_paint_ms": round((t_paint - t_big) * 1000, 1) if t_paint else None,
            "list_answer_to_last_change_ms": round((last - t_big) * 1000, 1) if last else None,
            "wheel_samples": len(frame),
            "wheel_ticks": 60,
            "wheel_to_paint_p50_ms": round(frame[len(frame) // 2], 2) if frame else None,
            "wheel_to_paint_p95_ms": round(frame[min(len(frame) - 1, int(len(frame) * 0.95))], 2) if frame else None,
            "cpu_s_60_wheel_ticks": round(cpu_seconds(shell) - c0, 3),
            "pss_mib_after": round(pss_mib(shell), 1),
        }
    finally:
        app.stop()
        shutil.rmtree(base, ignore_errors=True)


def scenario_output(x, kind):
    """A task printing 50 000 lines: task/exited → output panel stops changing."""
    base = Path(tempfile.mkdtemp(prefix=f"bench-out-{kind}-"))
    ws = make_workspace(base / "ws")
    data = base / "data"
    data.mkdir()
    seed_state(str(data), ws)
    tap = data / "tap.jsonl"
    app = App(kind, str(data), tap)
    try:
        win = x.wait_window(app.pids, 30)
        wait_tap(tap, "fs/list")
        g = win.get_geometry()
        x.wait_stable(x.region(win, 0, 0, 1, 1), 1.0, 20)
        # The command field sits in the output panel's bar (bottom area); click it, type, Enter.
        # Click the command field (in Qt, Ctrl+Shift+R typed into Monaco instead
        # of focusing the field during calibration, so input is by pointer for both).
        fx, fy = LAYOUT[kind]["command_field"]
        x.click(win, int(g.width * fx), int(g.height * fy))
        time.sleep(0.4)
        cmd = "seq 1 50000"
        for ch in cmd:
            x.key({" ": "space"}.get(ch, ch))
        region = x.region(win, *LAYOUT[kind]["output"])
        shell, _ = app.procs_split()
        c0 = cpu_seconds(shell)
        t0 = time.monotonic()
        x.key("Return")
        settled = x.wait_stable(region, 1.0, 60)
        cpu = cpu_seconds(shell) - c0
        return {
            "lines": 50000,
            "enter_to_output_settled_ms": round((settled - t0) * 1000, 1) if settled else None,
            "cpu_s": round(cpu, 3),
            "pss_mib_after": round(pss_mib(shell), 1),
        }
    finally:
        app.stop()
        shutil.rmtree(base, ignore_errors=True)


def git_rev():
    try:
        return subprocess.check_output(["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"], text=True).strip()
    except Exception:
        return None


def summarise(results, kinds):
    """Median of each repeated metric, so several repetitions are usable.

    A single sample of PSS or idle CPU on a busy host is not comparable run to
    run (observed spread: 54 MiB PSS, 0.1 pp idle CPU on the same build), so
    reports quote these medians and the sample count, not rep 0.
    """
    metrics = [
        "idle_workspace_open_pss_mib",
        "monaco_document_open_pss_mib",
        "after_typing_pss_mib",
        "idle_cpu_pct_10s",
        "idle_cpu_pct_10s_with_editor",
        "idle_cpu_pct_min",
        "idle_cpu_pct_min_with_editor",
    ]
    out = {}
    for kind in kinds:
        reps = results.get(kind, {}).get("memory_cpu_typing", [])
        if not reps:
            continue
        block = {}
        for m in metrics:
            values = [r[m] for r in reps if isinstance(r.get(m), (int, float))]
            if values:
                values.sort()
                block[m] = {"median": round(statistics.median(values), 2), "samples": len(values)}
        if block:
            out[kind] = block
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=5, help="startup repetitions per shell")
    ap.add_argument("--heavy-runs", type=int, default=3, help="repetitions of memory/explorer/output per shell")
    ap.add_argument("--only", choices=["qt", "tauri"])
    ap.add_argument("--scenario", default="all", choices=["all", "startup", "memory", "explorer", "output"])
    args = ap.parse_args()
    if os.environ.get("WAYLAND_DISPLAY") or os.environ.get("GDK_BACKEND") != "x11":
        raise SystemExit("refusing to run: not inside scripts/headless.sh (would open windows on the desktop)")
    x = XSession()
    kinds = [args.only] if args.only else ["qt", "tauri"]
    results = {
        "host": {
            "os": platform.platform(),
            "cpu": platform.processor() or platform.machine(),
            "cpus": os.cpu_count(),
            "mem_gib": round(psutil.virtual_memory().total / 2**30, 1),
            "display": "Xvfb 1440x900x24 (software rendering, no GPU); repaint observed by XGetImage sampling",
            "git": git_rev(),
            "date": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "loadavg_at_start": [round(v, 2) for v in os.getloadavg()],
        },
        "binaries": {},
    }
    for k, p in (("qt", QT_APP), ("tauri", TAURI_APP), ("axiomd", DAEMON)):
        results["binaries"][k] = {"path": str(p), "bytes": p.stat().st_size if p.exists() else None}
    # Interleaved: every repetition runs each shell back to back, so both see
    # the same background load. The 1-minute load average is recorded per sample.
    for kind in kinds:
        results[kind] = {"startup": {"cold": [], "warm": []}, "memory_cpu_typing": [], "explorer": [], "output": []}
    for rep in range(args.runs):
        for kind in (kinds if rep % 2 == 0 else list(reversed(kinds))):
            r = results[kind]
            load = os.getloadavg()[0]
            if args.scenario in ("all", "startup"):
                s1 = scenario_startup(x, kind, 1)
                for mode in ("cold", "warm"):
                    for row in s1[mode]:
                        row["loadavg_1m"] = round(load, 2)
                        r["startup"][mode].append(row)
            if args.scenario in ("all", "memory") and rep < args.heavy_runs:
                m = scenario_memory_cpu(x, kind)
                m["loadavg_1m"] = round(os.getloadavg()[0], 2)
                r["memory_cpu_typing"].append(m)
            if args.scenario in ("all", "explorer") and rep < args.heavy_runs:
                e = scenario_explorer(x, kind)
                e["loadavg_1m"] = round(os.getloadavg()[0], 2)
                r["explorer"].append(e)
            if args.scenario in ("all", "output") and rep < args.heavy_runs:
                o = scenario_output(x, kind)
                o["loadavg_1m"] = round(os.getloadavg()[0], 2)
                r["output"].append(o)
            print(f"[bench] rep {rep + 1}/{args.runs} {kind} done (load {load:.1f})", file=sys.stderr)
    results["medians"] = summarise(results, kinds)
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
