---
name: autopilot
description: Work through every open issue of jdrowne/RustyHGM on a recurring reminder, skipping issues with any given skip label. `start`, `restart`, `stop`; `--skip-label <label>` repeats.
argument-hint: start [--skip-label <label>]... | restart [--skip-label <label>]... | stop
allowed-tools: Bash, CronCreate, CronDelete, CronList
---

# Autopilot

Arguments: `$ARGUMENTS`

Load the Cron tools with `ToolSearch`.

## Work list

```text
gh issue list -R jdrowne/RustyHGM --state open --limit 10000 --json number,labels --jq '<filters> map(.number) | sort'
```

`<filters>` is `map(select(all(.labels[]; .name != "<label>"))) |` per skip
label, and empty without skip labels.

## Reminder

Cron `*/10 * * * *`, recurring. The prompt carries everything it needs, with
the work list command written out:

```text
REMINDER (autopilot) Skip labels: <labels>.
1. Run: <work list command>
2. Empty: load the Cron tools with ToolSearch, delete every job whose prompt starts with "REMINDER (autopilot)", tell the user.
3. Otherwise work each listed issue from the lowest number until it closes, passing over any whose last comment says it needs a person.
4. If one needs a person: comment why, add the label "<first>" to it, move on to the next.
```

`<first>` is the first skip label. Without skip labels, write `Skip labels:
none.` and leave the label clause out of step 4. Keep at most one such job.

## `start`

If a job exists, suggest `restart`. If the work list is empty, say so. Otherwise
create the reminder, report its ID, next run and seven-day expiry, and start
working.

## `restart`

Keep the skip labels from the old prompt unless given. Delete it, then `start`.

## `stop`

Delete every autopilot job, check `CronList`, report.
