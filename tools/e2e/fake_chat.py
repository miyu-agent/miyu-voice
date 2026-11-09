# 假的 OpenAI 兼容对话服务：POST /v1/chat/completions 记下请求体，流式回一段固定的回答（SSE）。
# FAKE_CHAT_TOOL=<工具名>：请求里带了工具、对话里还没有工具结果的，先叫这件工具（参数 {"text": ...}），拿到结果以后照常回字。
import http.server, json, os, sys, time

LOG = sys.argv[2]
REPLY = ["好的，", "三点见。", "明天", "再聊！"]


def sse(handler, chunk):
    handler.wfile.write(("data: " + json.dumps(chunk, ensure_ascii=False) + "\n\n").encode())
    handler.wfile.flush()


def chunk(delta, finish=None):
    out = {"id": "c1", "object": "chat.completion.chunk", "model": "fake",
           "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
    if finish:
        out["usage"] = {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    return out


class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        with open(LOG, "a") as f:
            f.write(json.dumps({"path": self.path, "messages": body.get("messages"),
                                "tools": [t["function"]["name"] for t in body.get("tools") or []]}, ensure_ascii=False) + "\n")
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        tool = os.environ.get("FAKE_CHAT_TOOL")
        has_result = any(m.get("role") == "tool" for m in body.get("messages", []))
        offered = [t["function"]["name"] for t in body.get("tools") or []]
        if tool and tool in offered and not has_result:
            call = {"index": 0, "id": "call_1", "type": "function",
                    "function": {"name": tool, "arguments": json.dumps({"text": "**你好**，我在。"}, ensure_ascii=False)}}
            sse(self, chunk({"tool_calls": [call]}))
            sse(self, chunk({}, "tool_calls"))
        else:
            for piece in REPLY:
                sse(self, chunk({"content": piece}))
                time.sleep(0.1)
            sse(self, chunk({}, "stop"))
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()

    def log_message(self, *a):
        pass


http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
