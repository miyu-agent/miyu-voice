# 同一个连接上先发一个长的 stt.transcribe、紧接着发 stt.stop，不等回应；印回应到的先后（BUS-2 第 2 条：后台答不排队）。
import json, socket, sys, time
home, wav = sys.argv[1], sys.argv[2]
s = socket.socket(socket.AF_UNIX); s.connect(open(f"{home}/run/socket").read().strip())
f = s.makefile("rw", encoding="utf-8")
def send(i, m, p):
    f.write(json.dumps({"jsonrpc": "2.0", "id": i, "method": m, "params": p}) + "\n"); f.flush()
def wait(i):
    while True:
        msg = json.loads(f.readline())
        if msg.get("id") == i: return msg
send("h", "hello", {"protocol": [1, 1], "head": {"kind": "e2e", "version": "0"}, "token": open(f"{home}/run/token").read().strip()}); wait("h")
send("p", "blob.put", {"path": wav}); blob = wait("p")["result"]["blob"]
t0 = time.time()
send("long", "bus.call", {"name": "stt", "method": "transcribe", "params": {"audio": blob, "language": "en"}})
send("stop", "bus.call", {"name": "stt", "method": "stop"})
for _ in range(2):
    while True:
        msg = json.loads(f.readline())
        if msg.get("id") in ("long", "stop"): break
    body = msg.get("result", msg.get("error"))
    print(f"{time.time()-t0:5.2f}s {msg['id']}: {json.dumps(body)[:90]}")
