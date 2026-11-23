# Security results

A scanner can write `~/Library/Application Support/aimon/security.json`. The Projects page matches entries to repositories by absolute path and shows a shield on a repository's card when it has secrets or dependency advisories, or when the scan is older than 10 days. `aimon security` prints the same results as a table.

```json
{"checked": "2026-11-17T09:00:00+01:00", "repos": [{"path": "/Users/name/code/app", "secrets": 0, "advisories": 2}]}
```
