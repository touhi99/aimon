# Design notes

Reasons behind rules in the code that names alone do not carry.

## Database

- Schema v3 added the `tools` table and cleared the stored file offsets once, so old logs are read again for tool calls. Calls are upserted, so rows are not counted twice.
- Schema v4 stores the input of each tool call and the reason it failed.
- One `calls` row holds ts, source, project, model, inp, out, cread, cw5, cw1, kind and `cost_override`.

## Claude Code logs

- A log file can vanish or become unreadable between listing and reading; it is skipped for that scan.
- A line without a trailing newline is still being written and is read again on the next scan.
- Streaming repeats a message id; the row with the most output tokens is kept.
- A call that reports its own cost uses it instead of the price list.
- Hourly token columns are Claude plan, Ollama plan, and API plus local.
- Without a Claude plan login, Claude Code is billed per token, so its usage counts as spend.
- A tool error arrives later as a `tool_result` with `is_error=true`, matched to the call by `tool_use` id.
- An error of just "Exit code N" gets the first line of output after it appended.
- MCP tool names `mcp__server__tool` are shown as `server:tool`.

## Project names

- A folder rule in `aliases.json` (a key starting with `/` or `~/`) covers everything under that folder; otherwise the last folder name is used.
- In repository mode the name is the nearest folder above the working directory with a `.git`. A folder outside any repository that holds repositories is a workspace, named `<folder> (workspace)`.

## Plan limits and Ollama

- The usage endpoint rate-limits with 429. Live limits are cached for 5 minutes, and after a failed request the next attempt waits 5 minutes, so refreshes do not keep the limit active.
- On failure the last live copy is shown, then the copy Claude Code keeps in `~/.claude.json`, both marked stale.
- A window whose `resets_at` has passed describes a finished period, so it is removed.
- The Keychain is asked at most once a day, so a missing item (API-key users) or a refused prompt is not asked again on every refresh.
- Ollama spend is signed with the ed25519 key that `ollama signin` writes. Only an unencrypted OpenSSH key is read. Any failure of the undocumented endpoint shows as "error". Session and weekly percentages are only available behind the browser login.

## Network

- `nettop` prints `name.pid`. The Claude Code binary is named after its version, for example `2.1.319`.
- Byte counts are `nettop`'s cumulative counters, diffed between runs. Hosts come from `lsof` and reverse DNS, cached, a few new lookups per run. macOS does not expose domain names without root.
- Hostnames of services the AI tools use are resolved forward every 6 hours, so their IPs get a real name.
- A recognised host belongs to a well-known provider or CDN. Recognised does not mean safe: a CDN hosts anyone's content.

## Projects and Use

- A program started outside launchd (opened from Finder after a clean quit) counts as running.
- `USE_GAP` is the number of seconds between two recorded uses of a key; the row count is the weight of a project's use.
- Foreground time is added every 15 seconds while a project's app is frontmost and the keyboard or mouse was used in the last two minutes.
- Use bars follow foreground time once any has been recorded, and opens before that. Shades use a square root, so small counts stay visible.
- App bundles are looked up in `/Applications`, `~/Applications` and up to three levels inside each repository.

## Menu bar app

- A second copy exits at launch with code 0, since it would add another menu bar item and repeat the log scanning. The LaunchAgent restarts the app only after a failure.
- The window gives the app a Dock icon, so Quit from the Dock or the app switcher only closes the window; the LaunchAgent would not restart a clean exit. The power button and a Quit with a reason (logout, restart, shutdown) still quit.
- A menu bar app gets no activation event, so opening the popover counts as a use of aimon.
- With plan limits off or no Claude plan found, the menu bar shows spend (or tokens) instead of an empty ring.
- Refreshes do not overlap and core commands run one after another, because each run scans logs and writes the same database.
- `aimon usage` runs git and Spotlight per project, so it is read at most every five minutes.
- The menu bar shows the 5-hour session, or the week when the week is the one about to run out.
- `--png` draws the real view in an off-screen window, because SwiftUI's `ImageRenderer` leaves `ScrollView` and `Picker` blank.

## Command line

- A closed stdout (`aimon ... | head`) exits quietly instead of printing a panic. Other panics and broken sockets keep the default behaviour.

## Proxy

- Requests ask the upstream for `Accept-Encoding: identity`, so usage can be read from the plain reply.
- Request logging is off, because paths and headers can hold secrets.
- A failure while recording usage is ignored, so counting does not break a request.
