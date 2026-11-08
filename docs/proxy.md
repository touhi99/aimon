# Proxy (optional, experimental)

For applications whose code cannot be changed, `proxy/aimon_proxy.py` is a local forwarding proxy (Python standard library only, listens on 127.0.0.1).
Start it with `python3 proxy/aimon_proxy.py` and point the application's base URL at it, with the project name in the path:

```
ANTHROPIC_BASE_URL=http://127.0.0.1:8787/p/my-app/anthropic
OPENAI_BASE_URL=http://127.0.0.1:8787/p/my-app/openai/v1
Ollama:     http://127.0.0.1:8787/p/my-app/ollama
LM Studio:  http://127.0.0.1:8787/p/my-app/lmstudio/v1
```

Requests are forwarded unchanged and responses are streamed back as they arrive. Token counts from each response are appended to `inbox/proxy.jsonl`.
API keys, headers and request bodies are not stored or logged. OpenAI-style streams include usage only when the client requests it
(`stream_options: {"include_usage": true}`). Add a provider with `--upstream name=https://host`.
These variables should not be set globally, since that would also route Claude Code through the proxy.

The proxy has been tested against a mock upstream only.
