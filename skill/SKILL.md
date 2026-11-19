---
name: aimon
description: Query AI Project Monitor (aimon) from the shell. Use for questions about network flows per process, API spend and tokens, Claude Code tool failures and sessions, which projects are used or stale, LaunchAgent and port state of projects, or security scan results.
---

# aimon CLI

`aimon` (in `~/.local/bin`) reads the same data as the menu bar app. Every command prints JSON or a table to stdout.
Run commands from any folder. Commands that scan first print `scanned N files, M usage rows` to stderr; ignore it.
The built-in usage text only lists `today|by-project|by-model|scan`; the other commands below exist too.

## Which command

| Question | Command |
|---|---|
| What network flows are happening? | `aimon net` |
| Spend today, per project | `aimon today` |
| Spend per project or model over N days | `aimon by-project N`, `aimon by-model N` |
| Spend this month, daily series, by app | `aimon month [offset]` (0 = this month, 1 = last) |
| Everything the app shows (spend, limits, cache hit) | `aimon snapshot` |
| Claude Code tool calls, failures, sessions (24 h) | `aimon trace` |
| Which projects are used, stale, untracked | `aimon usage` |
| Per project: services running or down, ports, unpushed work | `aimon projects` |
| Secret and advisory scan results | `aimon security` |
| Record a use of a project | `aimon use <folder\|thread> [--via web\|cli]` |
| Enable plan limits or Ollama spend | `aimon config <claude_limits\|ollama> <on\|off>` |

`aimon usage` is project usage (days used), not spend. Spend is `today`, `by-project`, `by-model`, `month`.

## Reading `aimon net`

Output is `{"procs":[...],"since":<unix ts>}`. Per process:

- `name`: `<binary> · <project folder>`, for example `python3.12 · my-app`.
- `in`, `out`: TCP bytes received and sent by the process since `since` (from `nettop`), including connections
  that have since closed, so a process with `conns: 0` can still have large totals.
- `conns` and `detail`: open outbound TCP connections right now, with `host`, `ip`, `port`, `up` (connection age) and `pid`.
- `ai`: the process is recognised as AI-related. `known`: the host is on aimon's list of expected hosts.
  `unknown` counts hosts that are not. Report `unknown > 0` or `known: false` first.
- `host` comes from reverse DNS, so it is often a CDN name (`*.akamaitechnologies.com`, `*.amazonaws.com`) and
  not the service. Say so when explaining it; do not guess the service from the CDN name alone.

It is a snapshot. To explain an entry, name the process, its project, the host, and what that project does (check
the project's README when unsure).

History: `~/Library/Application Support/aimon/aimon.db`, table `net(ts, proc, din, dout)` holds per-process byte
deltas per sample, with no hosts. Example:

```
sqlite3 -readonly "$HOME/Library/Application Support/aimon/aimon.db" \
  "select proc, sum(dout)/1e6 as mb_out from net where ts > strftime('%s','now','-1 day') group by proc order by 2 desc limit 10"
```

Other tables: `calls` (model calls: ts, project, model, tokens, session), `tools` (Claude Code tool calls: err, arg, msg),
`uses` (project use records).

## Notes

- Costs are estimates from `prices.json`. The provider's invoice is authoritative. Local models count as tokens only.
- Cost totals include Claude Code (`source` in `calls`) and anything logged to the inbox
  (`~/Library/Application Support/aimon/inbox/`, see `docs/inbox.md`).
- `trace` reads `~/.claude/projects` logs. `kinds_24h` splits failures into `blocked`, `error`, `exit`.
- Do not edit `aimon.db`. Open it read-only.
