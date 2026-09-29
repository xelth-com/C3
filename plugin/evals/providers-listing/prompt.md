---
name: providers-listing
description: Install test - the agent must use c3's own availability check to say which reviewers are usable right now and why, without inventing a verdict.
tags: [smoke, install]
runs: 1
max_turns: 10
timeout_seconds: 240
allowed_tools:
  - Read
  - Glob
  - Grep
  - Skill
  - Bash
---

Which reviewers can the c3 plugin use on this machine right now? Use c3's own availability
check (it makes no network call and writes nothing) and give me one line per provider with
its verdict and reason, plus the roster's pick if a roster exists. Do not create, print or
set any API key.
