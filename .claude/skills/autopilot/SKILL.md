---
name: autopilot
description: Keep Claude working autonomously on a GitHub issue via a recurring reminder. Sub-commands - `start <issue>` creates the reminder, `restart [issue]` recreates it, `stop` removes it.
argument-hint: start <issue> | restart [issue] | stop
allowed-tools: CronCreate, CronDelete, CronList
---

# Autopilot

Arguments: `$ARGUMENTS`

Parse the first word of the arguments as the sub-command and the rest as its argument.
The issue argument may be given as `42`, `#42`, or an issue URL; normalize it to the bare
issue number `X`.

The autopilot reminder is a recurring cron job (CronCreate) whose prompt is exactly:

```
Continue autonomously until issue {X} is solved.
```

An "autopilot job" is any job in `CronList` whose prompt matches
`Continue autonomously until issue <number> is solved.` There must never be more than one.

## `start <X>`

1. If `X` is missing, stop and tell the user: `Usage: /autopilot start <issue>`.
2. Run `CronList`. If an autopilot job already exists, do not create another one. Tell the
   user which issue it is for and suggest `/autopilot restart <X>` to replace it.
3. Otherwise call `CronCreate` with:
   - `cron`: `*/10 * * * *` (every 10 minutes)
   - `recurring`: `true`
   - `prompt`: `Continue autonomously until issue X is solved.` (with `X` substituted)
4. Report the job ID, the next run time from the tool result, and that recurring jobs are
   session-only and auto-expire after 7 days (use `restart` to renew).
5. Begin working on issue `X` right away; do not wait for the first reminder.

## `restart [X]`

1. Run `CronList` and find the autopilot job(s).
2. Determine the issue: use `X` if given, otherwise the issue number from the existing job.
   If neither exists, tell the user: `No autopilot running. Usage: /autopilot restart <issue>`.
3. `CronDelete` every existing autopilot job.
4. Create a fresh one exactly as in `start` step 3, and report as in `start` step 4.

## `stop`

1. Run `CronList` and `CronDelete` every autopilot job.
2. Report which issue(s) were stopped, or that no autopilot was running.

## Any other sub-command

Show usage: `/autopilot start <issue> | restart [issue] | stop`.

## When the issue is solved

When reminded and issue `X` is already solved, run the `stop` procedure instead of doing
more work, and tell the user autopilot has been turned off.
