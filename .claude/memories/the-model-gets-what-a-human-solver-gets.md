---
name: the-model-gets-what-a-human-solver-gets
description: "A polyglot task treats the model as Exercism treats a person: same files with the tests, told how to run them, no example answer, and a time limit that only stops a stuck run"
metadata:
  type: project
---

# The model gets what a human solver gets

A polyglot task treats the model the way Exercism treats a person solving the same exercise. The model starts with the files a solver downloads, test files included. It is told how to run the tests. It cannot reach the example answer or the approach write-ups. A time limit only stops a stuck run and never decides a result: an attempt that runs out of time is scored on what it had written.

**Why:** the exercises were written for solvers who read the expected names, error types and edge cases from the test file. Withholding what a person is given makes tasks fail for reasons that say nothing about the model or the coding harness.

**How to apply:**

- When a question comes up about what a task should show, allow or limit, look up what Exercism does for a human solver and do the same.
- Scoring always runs the original test files, so a change to a test never counts.
- Matching upstream is no argument here, per [Compatibility with upstream is not required](upstream-compatibility-is-not-required.md).
