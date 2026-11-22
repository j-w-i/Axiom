import json, subprocess, sys, tempfile, time, os
daemon, size = sys.argv[1], int(sys.argv[2])
ws = tempfile.mkdtemp(); data = tempfile.mkdtemp()
open(os.path.join(ws, 'min.js'), 'w').write('a' * size)  # one long line, every char matches
p = subprocess.Popen([daemon, '--data-dir', data] if False else [daemon], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     env={**os.environ, 'AXIOM_DATA_DIR': data})
def call(i, m, params):
    p.stdin.write((json.dumps({"jsonrpc":"2.0","id":i,"method":m,"params":params})+"\n").encode()); p.stdin.flush()
    while True:
        msg = json.loads(p.stdout.readline())
        if msg.get('id') == i: return msg
call(0, 'initialize', {"protocolVersion":"0.1","client":{"name":"audit","version":"0"}})
wid = call(1, 'workspace/open', {"path": ws})['result']['workspaceId']
t = time.time(); r = call(2, 'workspace/search', {"workspaceId": wid, "query": "a"})
el = time.time() - t
res = r.get('result') or r.get('error')
print(f"size={size} bytes: search took {el*1000:.0f} ms; result keys={list(res.keys()) if isinstance(res, dict) else res}; total={res.get('total') if isinstance(res, dict) else ''}")
p.kill()
