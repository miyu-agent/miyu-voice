# 当本机的头连核心（docs/reference/connection.md），照命令行参数发一串请求，一行印一个回应。
#   python3 -I head.py <数据根> '<方法> <JSON 参数>' …
import json, socket, sys

home = sys.argv[1]
path = open(f"{home}/run/socket").read().strip()
token = open(f"{home}/run/token").read().strip()
s = socket.socket(socket.AF_UNIX)
s.connect(path)
f = s.makefile("rw", encoding="utf-8")
n = 0

def call(method, params):
    global n
    n += 1
    rid = f"e2e-{n}"
    f.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method, "params": params}, ensure_ascii=False) + "\n")
    f.flush()
    while True:
        msg = json.loads(f.readline())
        if msg.get("id") == rid:
            return msg

call("hello", {"protocol": [1, 1], "head": {"kind": "e2e", "version": "0"}, "token": token})
for arg in sys.argv[2:]:
    method, _, params = arg.partition(" ")
    params = json.loads(params or "{}")
    # 「$blob」换成上一个 blob.put 回的编号。
    text = json.dumps(params)
    if "$blob" in text:
        text = text.replace("$blob", last_blob)
        params = json.loads(text)
    reply = call(method, params)
    if method == "blob.put" and "result" in reply:
        last_blob = reply["result"]["blob"]
    out = reply.get("result", reply.get("error"))
    print(method, json.dumps(out, ensure_ascii=False)[:300])
