# AI Monitor

A macOS menu bar app for monitoring AI tooling on a Mac: model usage and cost, Claude Code sessions and tool calls, and network traffic per process. It is meant for tracking cost, usage and traffic across different projects in a personal toolkit.

<p align="center">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="docs/popover-dark.png">
<img src="docs/popover-light.png" alt="The AI Monitor popover" width="420">
</picture>
</p>

## Features

The popover has three tabs.

- **Usage**: token counts and estimated API cost per project and per model, a daily and monthly spend chart, and Claude plan limits (5-hour session and weekly percentage). Optionally, Ollama cloud spend. Local models are counted as tokens only.
- **Trace**: Claude Code activity from its session logs. Tool calls in the last 24 hours with counts and failures, recent failures with the command or file involved, and recent sessions with duration, model calls, tool calls and failures.
- **Network**: outbound traffic per process, with open connections and remote hosts. AI-related processes are marked, and hosts that are not recognised are flagged. Hosts are resolved by reverse DNS, so entries are IP-level and not a full domain log.

## How it works

Usage comes from two sources:

1. **Claude Code.** Session logs in `~/.claude/projects` are read automatically. The project is the last folder of the working directory.
2. **The inbox.** Any other tool or application appends one JSON line per model call to a file in the inbox folder (see below).

Usage is stored in a local SQLite database, `~/Library/Application Support/aimon/aimon.db`. The Trace tab reads the same Claude Code logs. The Network tab samples `nettop` and `lsof` while the app runs.

## Requirements

- macOS 14 or later
- [Rust](https://rustup.rs)
- Swift toolchain (Xcode or the command line tools)

## Install

```
git clone <repository-url>
cd aimon
./build.sh        # builds the Rust core and the Swift app into AIMon.app
./install.sh      # installs a LaunchAgent: starts AI Monitor now and at login
```

`./uninstall.sh` stops AI Monitor and removes the LaunchAgent. The data folder is left in place.
Only one instance runs at a time; a second launch exits immediately.

## What AI Monitor reads and sends

- **Claude Code logs**: model name, token counts, timestamp and working folder of each call. Prompts and responses are not read.
- **Claude plan limits** (off by default): enable with the button in the popover or `aimon config claude_limits on`.
  When enabled, AI Monitor reads the Claude Code login token from the macOS Keychain and sends it to
  `api.anthropic.com/api/oauth/usage`, an undocumented endpoint that may change. macOS asks for permission to read the Keychain item;
  if it is refused or missing, AI Monitor does not ask again for a day. Disable with `aimon config claude_limits off`.
- **Ollama cloud spend** (off by default): enable with the button in the popover or `aimon config ollama on`.
  When enabled, and if `ollama signin` was run, AI Monitor signs a request to `ollama.com/api/usage` with the key in `~/.ollama/id_ed25519`.
  The key is used locally for signing and is not transmitted. This endpoint is also undocumented.
- **Network tab**: lists outbound connections per process using `nettop` and `lsof`. Nothing is sent anywhere.

Cost figures are estimates from a price list. The provider's invoice is authoritative.

## Report usage from a project

Append one JSON object per model call to any `*.jsonl` file in `~/Library/Application Support/aimon/inbox/`:

```json
{"project":"my-app","model":"gpt-5","in":1200,"out":300,"cost":0.004}
```

| Field | Required | Meaning |
|---|---|---|
| `project` | yes | Name shown in AI Monitor. If missing, the call is listed as `unassigned`. Use the same spelling every time. |
| `model` | yes | Model name, used for pricing and the by-model view. |
| `in`, `out` | yes | Input and output tokens. `in` counts uncached input only. |
| `cache_read`, `cache_write` | no | Cached input tokens read and written. |
| `cost` | no | Cost in USD, if already known. Overrides the price list. |
| `local` | no | `true` for models running locally (Ollama, LM Studio). Counted as tokens, not as spend. |
| `ts` | no | Unix seconds or an ISO date. Defaults to the file's modification time. |

Models listed in `prices.json` are priced automatically. For other models, pass `cost`; otherwise only tokens are shown.

From a shell:

```
aimon log --project my-app --model gpt-5 --in 1200 --out 300 --cost 0.004
```

Other flags: `--cache-read N`, `--cache-write N`, `--local`.

Files in the inbox are append-only. AI Monitor identifies each row by file name and line number, so removing or editing a line in the middle shifts the rows after it.

The reporting code should do nothing if `~/Library/Application Support/aimon` does not exist, and a failed write must not affect the application.

## Proxy (optional, experimental)

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

## Group folders into one project

Claude Code usage is named after the working folder. To count a repository and its subfolders as one project, add a folder rule to
`~/Library/Application Support/aimon/aliases.json`:

```json
{"~/code/my-repo": "my-repo", "old-folder-name": "my-repo"}
```

A key starting with `/` or `~/` matches everything under that folder and applies to new usage. A plain name renames an existing project in the reports.

## Prices

`prices.json` lists the Claude models. A file of the same shape at `~/Library/Application Support/aimon/prices.json` adds models and overrides entries.
A call can also carry its own `cost`.

## Verify

```
aimon by-project 1     # last day, grouped by project (API calls only)
aimon by-model 7
```

Local models and Claude Code plan usage are stored but are not included in these two reports.

## Screenshots

`AIMon.app/Contents/MacOS/AIMon --png out` writes `out-light.png` and `out-dark.png` and exits.
Set `HOME` to a scratch folder containing demo data to keep real usage out of the images.

## License

MIT. See `LICENSE`.
