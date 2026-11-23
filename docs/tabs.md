# Menu bar and window

The menu bar popover is the glance: a ring for the Claude plan's weekly use (API spend today when plan limits are off), cards for things to act on, and Open AI Monitor. A card appears for a stopped LaunchAgent, a plan limit at 85% or more, or a security finding. A click on a card opens the window at the page and project that shows it.

The window holds the history and the detail. It is dark, with a sidebar, and shows a Dock icon while open. Each number lives on one page; Overview's figures link to theirs.

## Overview

The plan ring, API spend this month, tokens in 7 days and failed tool calls in 24 hours, each a link to its page. Below, the attention cards and three cards for the last 8 weeks, from `aimon pace`:

- Agent works alone: the 90th percentile of run length, the time from a prompt to the agent's last reply before the next prompt, read from `~/.claude/projects`.
- Corrections: the share of user messages tagged as pushback in Backtrail's `turn_tags` table, filled by `skill/corrections.sh` in the Backtrail repository.
- Projects kept alive: repositories with commits on 5 or more days in the last 28.

## Projects

Every repository of a Backtrail thread that is not dropped, plus every other git repository at the top of the workspace, grouped as in development, built, and parked and other. A card shows the repository's commit days in the last 28 as a strip (orange below 5 days), its running service and port, push state, time in its app in the last 7 days and a shield for a security finding. A click opens one panel under the group: commit days, time in its app, services, git state, stopped agents, the security finding and a year of days in its app.

Data sources:

- `config.json` and `backtrail.db` (read-only) in `~/Library/Application Support/Backtrail`. The `repos` map gives each repository's folder and thread; the thread state comes from the database.
- A LaunchAgent in `~/Library/LaunchAgents` belongs to a repository when its working directory or program is inside it. Agents with `KeepAlive` are counted; scheduled agents are idle most of the time.
- A listening port belongs to a repository when the working directory of its process is inside it. Processes of other users are not visible.
- Time in an app is stored in the `uses` and `spent` tables, at most once per project per 10 minutes, when an app bundle that belongs to the project becomes the active app, when the popover opens (for aimon itself), or when a tool runs `aimon use <repository folder|thread name> [--via web|cli]`. A bundle belongs to a project when it is inside the repository, when its name matches the repository folder, or when the `apps` map in `config.json` names it (`{"apps": {"Demo.app": "demo/demo-app"}}`).

## Spend

Claude plan limits (5-hour session and weekly percentage), API spend per day with earlier months, tokens per day for 14 days, the 7-day split by model, and tokens and estimated API cost per project for 7 days: the top 6, then the rest on request. A click on a project shows its tokens per day. Ollama cloud spend is optional. Local models are counted as tokens.

## Activity

Claude Code activity from its session logs: failed tool calls per day for 7 days, split into command exits, blocked calls and tool errors; the tool mix of the last 24 hours; and the sessions of the selected day. A click on a day selects it; a click on a session shows its model, tools and failures with the command or file involved.

## Network

Outbound traffic per process in 24 hours, as bars for bytes sent and received. AI-related processes are marked, and processes with unrecognised hosts are orange. A click shows open connections and hosts. Hosts are resolved by reverse DNS, so entries are IP-level.
