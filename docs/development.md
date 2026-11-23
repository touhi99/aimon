# Development

## Check totals

```
aimon by-project 1     # last day, grouped by project (API calls only)
aimon by-model 7
```

Local models and Claude Code plan usage are stored but are not included in these two reports.

## Screenshots

`AIMon.app/Contents/MacOS/AIMon --png out` writes `out-light.png` and `out-dark.png` and exits.
`AIMON_WINDOW=1` renders the window instead of the popover. `AIMON_TAB` picks its page (0 Spend, 1 Activity, 2 Network, 3 Projects, 6 Overview) and `AIMON_OPEN` opens one project's panel by name.
Set `HOME` to a scratch folder containing demo data to keep real usage out of the images.
