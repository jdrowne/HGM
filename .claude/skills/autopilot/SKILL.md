---
name: autopilot
description: Work through the open issues of jdrowne/RustyHGM on a recurring reminder, skipping issues with a skip label. `start [issue]`, `restart [issue]`, `stop`; `--skip-label <label>` repeats, default `question`.
argument-hint: start [issue] [--skip-label <label>]... | restart [issue] [--skip-label <label>]... | stop
allowed-tools: Bash, CronCreate, CronDelete, CronList
---

# Autopilot

Arguments: `$ARGUMENTS`

Load the Cron tools with `ToolSearch`. `X` is the given issue number.

## Work list

```text
gh issue list -R jdrowne/RustyHGM --state open --limit 10000 --json number,labels --jq '<filters> map(.number) | sort'
```

`<filters>` is `map(select(all(.labels[]; .name != "<label>"))) |` per skip label.

## Reminder

Cron `*/10 * * * *`, recurring, prompt:

```text
REMINDER (autopilot): follow "When a reminder fires" in .claude/skills/autopilot/SKILL.md. Priority: #X. Skip labels: <labels>.
```

Omit `Priority: #X.` without `X`. Keep at most one such job.

## When a reminder fires

1. Empty work list: run `stop` and tell the user.
2. Work on `X` if listed, else the lowest number, until it closes.
3. Needs a person: add the first skip label, comment why, move on.

## `start`

If a job exists, suggest `restart`. If the work list is empty, say so. Otherwise
create the reminder, report its ID, next run and seven-day expiry, and start
working.

## `restart`

Keep `X` and labels from the old prompt unless given. Delete it, then `start`.

## `stop`

Delete every autopilot job, check `CronList`, report.
