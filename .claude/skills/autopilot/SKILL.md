---
name: autopilot
description: Work through the open issues of jdrowne/RustyHGM on a recurring reminder, skipping issues with a skip label. `start [issue]`, `restart [issue]`, `stop`; `--skip-label <label>` repeats, default `needs decision`.
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

Cron `*/10 * * * *`, recurring. The prompt carries everything it needs, with
the work list command written out and `<first>` the first skip label:

```text
REMINDER (autopilot) Priority: #X. Skip labels: <labels>.
1. Run: <work list command>
2. Empty: load the Cron tools with ToolSearch, delete every job whose prompt starts with "REMINDER (autopilot)", tell the user.
3. Otherwise work on #X if listed, else the lowest number, until it closes.
4. If it needs a person: add the label "<first>" to it, comment why, move on to the next.
```

Omit `Priority: #X.` without `X`, and the `#X if listed, else` wording with it.
Keep at most one such job.

## `start`

If a job exists, suggest `restart`. If the work list is empty, say so. Otherwise
create the reminder, report its ID, next run and seven-day expiry, and start
working.

## `restart`

Keep `X` and labels from the old prompt unless given. Delete it, then `start`.

## `stop`

Delete every autopilot job, check `CronList`, report.
