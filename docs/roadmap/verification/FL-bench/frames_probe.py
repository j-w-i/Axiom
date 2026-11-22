#!/usr/bin/env python3
"""Idle probe (FL-PERF-001). It launches a Flutter profile bundle on the current, private DISPLAY,
with `main.rs` restored or with only the workspace. Over N seconds it reports per-thread CPU from
/proc and the engine's frame count from the VM service's timeline.

Run it beside the bench harness: `run.py` from the tag tauri-shell-final, with
../M5-bench/run.py.diff applied, supplies make_workspace and seed_state. Run it under
scripts/headless.sh with AXIOMD_UNDER_TEST set:

    frames_probe.py <bundle/axiom> [seconds] [with-doc 0|1]    PROBE_SETTLE=<s> waits first (default 8)
"""
import base64, json, os, re, socket, struct, subprocess, sys, tempfile, time
from pathlib import Path
from urllib.parse import urlparse

sys.path.insert(0, str(Path(__file__).parent))
import run  # the bench harness: make_workspace, seed_state, DAEMON

app_bin = sys.argv[1]
secs = float(sys.argv[2]) if len(sys.argv) > 2 else 10
with_doc = (sys.argv[3] if len(sys.argv) > 3 else "1") == "1"


class WS:
    def __init__(self, url):
        u = urlparse(url)
        self.s = socket.create_connection((u.hostname, u.port))
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall((f"GET {u.path} HTTP/1.1\r\nHost: {u.hostname}:{u.port}\r\nUpgrade: websocket\r\n"
                        f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(4096)
        self.buf = buf.split(b"\r\n\r\n", 1)[1]
        self.id = 0

    def _read(self, n):
        while len(self.buf) < n:
            c = self.s.recv(1 << 20)
            if not c:
                raise EOFError
            self.buf += c
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def recv(self):
        data = b""
        while True:
            b0, b1 = self._read(2)
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._read(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._read(8))[0]
            data += self._read(n)
            if b0 & 0x80:
                return data

    def call(self, method, **params):
        self.id += 1
        payload = json.dumps({"jsonrpc": "2.0", "id": str(self.id), "method": method, "params": params}).encode()
        mask = os.urandom(4)
        hdr = bytes([0x81])
        n = len(payload)
        hdr += bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n)
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(payload)))
        while True:
            m = json.loads(self.recv())
            if m.get("id") == str(self.id):
                if "error" in m:
                    raise RuntimeError(m["error"])
                return m["result"]


def thread_cpu(pid):
    out = {}
    for t in Path(f"/proc/{pid}/task").iterdir():
        try:
            name = (t / "comm").read_text().strip()
            f = (t / "stat").read_text().rsplit(")", 1)[1].split()
            out[(t.name, name)] = (int(f[11]) + int(f[12])) / os.sysconf("SC_CLK_TCK")
        except (FileNotFoundError, ProcessLookupError):
            pass
    return out


base = Path(tempfile.mkdtemp(prefix="probe-"))
ws_dir = run.make_workspace(base / "ws")
data = base / "data"
data.mkdir()
run.seed_state(str(data), ws_dir, open_files=["main.rs"] if with_doc else ())
env = dict(os.environ, XDG_CONFIG_HOME=str(data / "config"), XDG_DATA_HOME=str(data / "share"))
proc = subprocess.Popen([app_bin, "--daemon", str(run.DAEMON), "--data-dir", str(data)], env=env,
                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
uri = None
deadline = time.monotonic() + 30
while time.monotonic() < deadline and uri is None:
    line = proc.stdout.readline()
    m = re.search(r"(http://127\.0\.0\.1:\d+/[^\s]*)", line)
    if m:
        uri = m.group(1)
if uri is None:
    sys.exit("no VM service URI")
wsurl = uri.replace("http://", "ws://").rstrip("/") + "/ws"
time.sleep(float(os.environ.get("PROBE_SETTLE", "8")))  # settle: startup, the restored document, the first highlight
vm = WS(wsurl)
vm.call("setVMTimelineFlags", recordedStreams=["Embedder", "Dart", "GC"])
vm.call("clearVMTimeline")
c0 = thread_cpu(proc.pid)
time.sleep(secs)
c1 = thread_cpu(proc.pid)
tl = vm.call("getVMTimeline")
names = {}
for e in tl["traceEvents"]:
    if e.get("ph") in ("B", "X", "b"):
        names[e["name"]] = names.get(e["name"], 0) + 1
total = sum(c1.get(k, 0) - v for k, v in c0.items())
print(f"with_doc={with_doc} window={secs}s total_cpu={total / secs * 100:.1f}%")
byname = {}
for k, v in c0.items():
    byname[k[1]] = byname.get(k[1], 0) + c1.get(k, v) - v
for n, v in sorted(byname.items(), key=lambda kv: -kv[1])[:8]:
    print(f"  thread {n:20} {v / secs * 100:6.1f}%")
for n, c in sorted(names.items(), key=lambda kv: -kv[1])[:15]:
    print(f"  event {n:40} {c}")
os.killpg(proc.pid, 15)
proc.wait(5)
