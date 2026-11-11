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
1. List the open issues that do not need a decision: gh issue list -R jdrowne/RustyHGM --state open --limit 10000 --json number,labels --jq 'map(select(all(.labels[]; .name != "needs decision"))) | map(.number) | sort'
2. If the list is empty, delete this reminder with CronList and CronDelete, and tell me.
3. Otherwise work each issue, lowest number first, until it closes. If one needs a decision from me, add the "needs decision" label to it, comment explaining the decision, and move on to the next.
```

For each `--skip-label`, add a space and `and .name != "LABEL"` right after
`.name != "needs decision"`.

Report the reminder's ID, next run and seven-day expiry.

## `restart`

Delete the old reminder, then `start`, reusing its skip labels unless new ones
are given.

## `stop`

Delete every autopilot reminder, check `CronList`, report.
