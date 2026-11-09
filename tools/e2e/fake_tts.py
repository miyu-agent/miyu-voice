# 假的 OpenAI 兼容合成服务：POST /v1/audio/speech 记下请求体，回一段 0.1 秒的静音 wav。
import http.server, json, os, struct, sys, time

LOG = sys.argv[2]

def wav():
    rate = 16000
    n = int(rate * float(sys.argv[3] if len(sys.argv) > 3 else 0.1))
    data = b"\0\0" * n
    return b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16) + b"data" + struct.pack("<I", len(data)) + data

class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        with open(LOG, "a") as f:
            f.write(json.dumps({"path": self.path, "auth": self.headers.get("Authorization"), "body": json.loads(body)}, ensure_ascii=False) + "\n")
        time.sleep(float(os.environ.get("TTS_DELAY", "0")))
        out = wav()
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)
    def log_message(self, *a):
        pass

http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
