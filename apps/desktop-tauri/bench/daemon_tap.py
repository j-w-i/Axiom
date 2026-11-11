#!/usr/bin/env python3
"""Transparent stdio tap around axiomd, used only by the benchmarks.

Both shells spawn "the daemon" and speak protocol 0.1 to it, so a tap that
forwards bytes unchanged and records *when* protocol events pass gives one
shell-agnostic clock for both Qt and Tauri:

  - t_spawn:        the shell started the daemon (tap process start)
  - t_initialize:   the shell sent `initialize`
  - t_ready:        the daemon answered `initialize` (the shell can show Connected)
  - t_first_list:   the first `fs/list` answer went back (explorer can render)
  - t_first_read:   the first `fs/readFile` answer went back (editor can render)

Usage (set by bench/run.py): AXIOM_TAP_LOG=<file> AXIOM_TAP_REAL=<axiomd> daemon_tap.py <args>
It never writes to stdout except the daemon's own bytes (stdout is the protocol).
"""
import json
import os
import subprocess
import sys
import threading
import time

T0 = time.monotonic()
log_path = os.environ["AXIOM_TAP_LOG"]
real = os.environ["AXIOM_TAP_REAL"]
lock = threading.Lock()
pending = {}  # id -> method


def record(event, **kw):
    with lock, open(log_path, "a", encoding="utf-8") as f:
        f.write(json.dumps({"event": event, "t": time.monotonic(), "wall": time.time(), **kw}) + "\n")


record("spawn", t_tap_start=T0, ppid=os.getppid())
proc = subprocess.Popen([real, *sys.argv[1:]], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
record("daemon_pid", pid=proc.pid)


def pump_in():
    src = sys.stdin.buffer
    for line in iter(src.readline, b""):
        try:
            msg = json.loads(line)
            if "id" in msg and "method" in msg:
                pending[msg["id"]] = msg["method"]
                if msg["method"] in ("initialize", "fs/list", "fs/readFile", "fs/writeFile", "workspace/open"):
                    record("request", method=msg["method"], id=msg["id"])
        except Exception:
            pass
        proc.stdin.write(line)
        proc.stdin.flush()
    proc.stdin.close()


def pump_out():
    dst = sys.stdout.buffer
    for line in iter(proc.stdout.readline, b""):
        dst.write(line)
        dst.flush()
        try:
            msg = json.loads(line)
            if "id" in msg and "method" not in msg:
                m = pending.pop(msg["id"], None)
                if m in ("initialize", "fs/list", "fs/readFile", "fs/writeFile", "workspace/open"):
                    record("response", method=m, id=msg["id"], bytes=len(line))
        except Exception:
            pass


threading.Thread(target=pump_in, daemon=True).start()
out = threading.Thread(target=pump_out, daemon=True)
out.start()
code = proc.wait()
out.join(timeout=2)
record("exit", code=code)
sys.exit(code)
