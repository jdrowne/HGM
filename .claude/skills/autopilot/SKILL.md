---
name: autopilot
description: Work through every open issue of jdrowne/RustyHGM on a reminder every 10 minutes, skipping issues with any given skip label. `start`, `restart`, `stop`; `--skip-label <label>` repeats.
argument-hint: start [--skip-label <label>]... | restart [--skip-label <label>]... | stop
allowed-tools: Bash, CronCreate, CronDelete, CronList
---

# Autopilot

Load `CronCreate`, `CronDelete` and `CronList` with `ToolSearch`.

## `start`

If an autopilot reminder exists, suggest `restart`. Otherwise create a recurring
`*/10 * * * *` reminder with exactly this prompt, then start working on it:

```text
REMINDER (autopilot)
1. List the open issues: gh issue list -R jdrowne/RustyHGM --state open --limit 10000 --json number,labels --jq 'map(.number) | sort'
2. If the list is empty, delete this reminder with CronList and CronDelete, and tell me.
3. Otherwise work each issue, lowest number first, until it closes. Pass over any whose last comment says it needs a person. If one needs a person, comment why and move on.
```

If `--skip-label` is given, put this in front of `map(.number)`, with one
`.name != "..."` per given label, joined with `and`:

```text
map(select(all(.labels[]; .name != "LABEL1" and .name != "LABEL2"))) |
```

Report the reminder's ID, next run and seven-day expiry.

## `restart`

Delete the old reminder, then `start`, reusing its skip labels unless new ones
are given.

## `stop`

Delete every autopilot reminder, check `CronList`, report.
