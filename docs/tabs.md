# Tabs

## Spend

Token counts and estimated API cost per project and per model, a daily and monthly chart, and Claude plan limits (5-hour session and weekly percentage). Ollama cloud spend is optional. Local models are counted as tokens.

## Trace

Claude Code activity from its session logs: tool calls in the last 24 hours with counts and failures, recent failures with the command or file involved, and recent sessions with duration, model calls, tool calls and failures.

## Network

Outbound traffic per process, with open connections and remote hosts. AI-related processes are marked and unrecognised hosts are flagged. Hosts are resolved by reverse DNS, so entries are IP-level.

## Projects

Shown when Backtrail is installed. One row per repository of a Backtrail thread whose state is `moving`: its LaunchAgents and whether they run, the TCP ports its processes listen on, and unpushed or uncommitted work. The menu bar item gets a red dot while a LaunchAgent that should run is not running.

Data sources:

- `config.json` and `backtrail.db` (read-only) in `~/Library/Application Support/Backtrail`. The `repos` map gives each repository's folder and thread; the thread state comes from the database.
- A LaunchAgent in `~/Library/LaunchAgents` belongs to a repository when its working directory or program is inside it. Agents with `KeepAlive` are counted; scheduled agents are idle most of the time.
- A listening port belongs to a repository when the working directory of its process is inside it. Processes of other users are not visible.

The tab has been checked on one Mac.

## Use

Shown with Projects. Every project of a Backtrail thread that is not dropped, grouped as in use (used in the last 7 days), going stale (unused for over a week, or for two weeks while it still got commits), not used lately, and not used yet. Each project has a 12-week strip, one square per week, darker for more days used. A click opens the last year as a day grid.

Use is stored in the `uses` table, at most once per project per hour, when:

- An app bundle that belongs to a project becomes the active app. A bundle belongs to a project when it is inside the repository, when its name matches the repository folder, or when the `apps` map in `config.json` names it (`{"apps": {"Kestrel.app": "kestrel/kestrel-app"}}`). Before the first recorded use, the bundle's last-used date from Spotlight is shown.
- The popover is opened (for aimon itself).
- A tool runs `aimon use <repository folder|thread name> [--via web|cli]`. Repository folders listed under `web` in `config.json` (`{"web": ["my-site"]}`) count as tracked before their first recorded use.

Projects with no app and no recorded use are listed as not tracked.
