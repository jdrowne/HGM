---
name: upstream-compatibility-is-not-required
description: RustyHGM does not have to stay compatible with upstream metauto-ai/HGM; matching upstream is never by itself a reason to keep or reject a change
metadata:
  node_type: memory
  type: project
  originSessionId: 6eef50dc-ed68-4933-9ba6-851920aa3abe
  modified: 2026-11-12T07:43:14.553Z
---

# Compatibility with upstream is not required

RustyHGM does not have to stay compatible with the upstream metauto-ai/HGM. Its flags, file and folder formats, prompts, Docker images, package lists and results may all differ from upstream's.

**Why:** RustyHGM is a program of its own. Holding it to what upstream does would keep upstream's defects and block improvements that are worth making here.

**How to apply:**

- That upstream does something a certain way is not by itself a reason to keep it, and that a change departs from upstream is not a reason to reject it or to water it down. Judge the change on what it does for RustyHGM.
- Do not add aliases, converters or fallbacks whose only purpose is to accept or produce what upstream does.
- An issue or commit does not need to defend a difference from upstream. A fixture recorded from upstream is updated when the code it checks is changed on purpose.
- Upstream is still worth reading to learn how something works.
- This is separate from what a benchmark's own tests need. Those needs still hold: see [Always use latest versions](always-use-latest-versions.md).

Related: [Work only on this repo](work-only-on-this-repo.md).
