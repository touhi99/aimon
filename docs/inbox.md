# Reporting usage

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
