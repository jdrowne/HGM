---
name: autopilot
description: Keep Claude working autonomously through the open issues of jdrowne/RustyHGM via a recurring reminder, leaving issues with a skip label to a person, and turn the reminder off once no issue is left. Sub-commands - `start [issue]` creates the reminder, `restart [issue]` recreates it, `stop` removes it; `start` and `restart` also take `--skip-label <label>`, repeatable, which defaults to `question`.
argument-hint: start [issue] [--skip-label <label>]... | restart [issue] [--skip-label <label>]... | stop
allowed-tools: Bash, CronCreate, CronDelete, CronList
---

# Autopilot

Arguments: `$ARGUMENTS`

Fetch `CronCreate`, `CronList` and `CronDelete` with `ToolSearch` first.

Parse the first word of the arguments as the sub-command. The rest may name one
issue, as `42`, `#42` or an issue URL, normalized to the bare number `X`, and any
number of `--skip-label <label>` pairs, quoted when the label holds a space.
When no `--skip-label` is given, the skip labels are `question` alone.

## The loop command

Each skip label `<label>` puts `map(select(all(.labels[]; .name != "<label>"))) |`
and a space at the very start of the `--jq` program. With the default label the
command is:

```text
gh issue list -R jdrowne/RustyHGM --state open --limit 10000 --json number,title,labels --jq 'map(select(all(.labels[]; .name != "question"))) | sort_by(.number) | map({number, title, labels: [.labels[].name]})'
```

A label is never left out through `--label` or `--search`: either sends the
listing through GitHub's search, which returns at most 1000 issues.

## The reminder

The autopilot reminder is one recurring cron job (CronCreate). Its prompt is the
following text, with the loop command substituted for `{COMMAND}`. The first
sentence in brackets is present only when an issue `X` was given, and the
second sentence in brackets is repeated once for each skip label:

```text
REMINDER (autopilot): Continue autonomously on the open issues of jdrowne/RustyHGM. Run {COMMAND} for the open issues that are this loop's work, lowest number first. [Finish issue #{X} first while it is listed.] Otherwise take the lowest-numbered issue it lists, and run the command again when that issue closes. [An issue labelled '{label}' is left to a person, whatever else it carries.] When you need a person's decision on an issue, add the label 'question' to it, explain what is needed in a comment, and move on to the next issue. When the command lists no issue, the work is done: run the autopilot stop procedure and tell the user that autopilot has turned itself off.
```

An "autopilot job" is any job in `CronList` whose prompt starts with
`REMINDER (autopilot):`, or with `Continue autonomously until issue`, the form
earlier versions of this skill wrote. There must never be more than one.

## `start`

1. Run `CronList`. If an autopilot job already exists, do not create another
   one. Show the user its prompt's issue and skip labels, and suggest
   `/autopilot restart` to replace it.
2. Run the loop command. If it lists no issue, tell the user that there is no
   work for autopilot, name the label that holds back each open issue, and stop
   without scheduling anything.
3. Otherwise call `CronCreate` with `cron` `*/10 * * * *`, `recurring` `true`
   and the reminder as `prompt`.
4. Report the job ID, the next run time from the tool result, the skip labels,
   and that recurring jobs live in this session only and auto-expire after
   seven days (use `restart` to renew).
5. Begin working right away on issue `X`, or on the lowest-numbered issue the
   command lists; do not wait for the first reminder.

## `restart`

1. Run `CronList` and find the autopilot job or jobs.
2. Determine the issue and the skip labels: those given, otherwise those in the
   existing job's prompt. An earlier-version job carries no skip labels, so the
   default `question` applies.
3. `CronDelete` every existing autopilot job.
4. Continue as in `start` from step 2.

## `stop`

1. Run `CronList` and `CronDelete` every autopilot job.
2. Call `CronList` again to confirm none is left.
3. Report which job was stopped, or that no autopilot was running.

## When a reminder fires

Run the loop command first. When it lists no issue, run `stop` instead of doing
more work, and tell the user that autopilot has turned itself off because no
open issue is left that is not labelled with a skip label.

## Any other sub-command

Show the usage from `argument-hint`.
