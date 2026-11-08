#!/usr/bin/env python3
"""Optional AI Monitor proxy: track an app's AI usage by changing only its base URL. Standard library only.

    python3 aimon_proxy.py                 # listens on 127.0.0.1:8787

Point an app at  http://127.0.0.1:8787/p/<project>/<provider>  instead of the provider's address:

    ANTHROPIC_BASE_URL=http://127.0.0.1:8787/p/my-app/anthropic     (the SDK adds /v1/messages)
    OPENAI_BASE_URL=http://127.0.0.1:8787/p/my-app/openai/v1
    Ollama:  http://127.0.0.1:8787/p/my-app/ollama      LM Studio: http://127.0.0.1:8787/p/my-app/lmstudio/v1

Requests are forwarded unchanged, API keys included, and answers are streamed back as they arrive. The token counts
in each answer become one line in AI Monitor's inbox (proxy.jsonl). Keys, headers and bodies are never stored or logged.
OpenAI-style streams only carry usage when the app asks for it (stream_options: {"include_usage": true}).
Add or change providers with  --upstream name=https://host  (repeatable).
"""
import argparse, http.client, json, threading, time, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

UPSTREAMS = {"anthropic": "https://api.anthropic.com", "openai": "https://api.openai.com",
             "ollama": "http://127.0.0.1:11434", "lmstudio": "http://127.0.0.1:1234"}
LOCAL = {"ollama", "lmstudio"}
INBOX = Path.home() / "Library/Application Support/aimon/inbox"
HOP = {"connection", "keep-alive", "proxy-authenticate", "proxy-authorization", "te", "trailers", "transfer-encoding",
       "upgrade", "content-length", "host", "accept-encoding"}
MAX_KEPT = 16 * 1024 * 1024
lock = threading.Lock()


def objects(body: bytes):
    """JSON objects in a reply: one document, server-sent events, or one object per line."""
    text = body.decode("utf-8", "replace")
    try:
        yield json.loads(text); return
    except ValueError:
        pass
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("data:"):
            line = line[5:].strip()
        if line.startswith("{"):
            try: yield json.loads(line)
            except ValueError: pass


def usage_of(body: bytes):
    """(model, in, out, cache_read, cache_write) from a reply; the largest value seen for each field wins."""
    model, got = None, {"in": 0, "out": 0, "cr": 0, "cw": 0}
    for o in objects(body):
        if not isinstance(o, dict): continue
        msg = o.get("message") if isinstance(o.get("message"), dict) else {}
        model = o.get("model") or msg.get("model") or model
        u = o.get("usage") or msg.get("usage") or {}
        if "prompt_tokens" in u:                                   # OpenAI-style
            cached = (u.get("prompt_tokens_details") or {}).get("cached_tokens") or 0
            cand = {"in": max(0, (u.get("prompt_tokens") or 0) - cached), "out": u.get("completion_tokens") or 0, "cr": cached, "cw": 0}
        elif "eval_count" in o or "prompt_eval_count" in o:        # Ollama
            cand = {"in": o.get("prompt_eval_count") or 0, "out": o.get("eval_count") or 0, "cr": 0, "cw": 0}
        else:                                                      # Anthropic-style
            cand = {"in": u.get("input_tokens") or 0, "out": u.get("output_tokens") or 0,
                    "cr": u.get("cache_read_input_tokens") or 0, "cw": u.get("cache_creation_input_tokens") or 0}
        for k in got: got[k] = max(got[k], int(cand[k] or 0))
    return model, got["in"], got["out"], got["cr"], got["cw"]


def record(project, provider, body):
    try:
        model, i, o, cr, cw = usage_of(body)
        if i + o + cr + cw == 0: return
        line = {"ts": int(time.time()), "project": project, "model": model or "?", "in": i, "out": o, "cache_read": cr, "cache_write": cw}
        if provider in LOCAL: line["local"] = True
        with lock:
            INBOX.mkdir(parents=True, exist_ok=True)
            with open(INBOX / "proxy.jsonl", "a") as f: f.write(json.dumps(line) + "\n")
    except Exception:
        pass   # counting must never break a request


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a): pass          # no request logging: paths and headers can hold secrets

    def fail(self, code, msg):
        data = json.dumps({"error": msg}).encode()
        self.send_response(code); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data))); self.send_header("Connection", "close"); self.end_headers()
        self.wfile.write(data); self.close_connection = True

    def handle_any(self):
        parts = urllib.parse.urlsplit(self.path)
        seg = parts.path.split("/", 4)               # ['', 'p', project, provider, rest]
        if len(seg) < 4 or seg[1] != "p" or seg[3] not in UPSTREAMS:
            return self.fail(404, "use /p/<project>/<provider>/..., providers: " + ", ".join(UPSTREAMS))
        project, provider = urllib.parse.unquote(seg[2]), seg[3]
        rest = "/" + (seg[4] if len(seg) > 4 else "")
        up = urllib.parse.urlsplit(UPSTREAMS[provider])
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else None
        headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP}
        headers["Host"] = up.netloc; headers["Accept-Encoding"] = "identity"   # plain text, so usage can be read
        try:
            cls = http.client.HTTPSConnection if up.scheme == "https" else http.client.HTTPConnection
            conn = cls(up.netloc, timeout=600)
            conn.request(self.command, up.path.rstrip("/") + rest + (("?" + parts.query) if parts.query else ""), body, headers)
            r = conn.getresponse()
        except Exception:
            return self.fail(502, "could not reach " + provider)
        self.send_response(r.status)
        for k, v in r.getheaders():
            if k.lower() not in HOP: self.send_header(k, v)
        self.send_header("Connection", "close"); self.end_headers()
        kept, size = [], 0
        try:
            while True:
                chunk = r.read1(65536) if hasattr(r, "read1") else r.read(65536)
                if not chunk: break
                self.wfile.write(chunk); self.wfile.flush()
                if size < MAX_KEPT: kept.append(chunk); size += len(chunk)
        except Exception:
            pass
        finally:
            conn.close(); self.close_connection = True
        if 200 <= r.status < 300: record(project, provider, b"".join(kept))

    do_GET = do_POST = do_PUT = do_DELETE = handle_any


def main():
    ap = argparse.ArgumentParser(description="AI Monitor usage proxy")
    ap.add_argument("--port", type=int, default=8787)
    ap.add_argument("--upstream", action="append", default=[], metavar="NAME=URL")
    a = ap.parse_args()
    for item in a.upstream:
        name, _, url = item.partition("="); UPSTREAMS[name] = url.rstrip("/")
    srv = ThreadingHTTPServer(("127.0.0.1", a.port), Handler)   # this machine only
    srv.daemon_threads = True
    print(f"AI Monitor proxy on http://127.0.0.1:{a.port}  (Ctrl-C to stop)", flush=True)
    try: srv.serve_forever()
    except KeyboardInterrupt: pass


if __name__ == "__main__":
    main()
