---
name: command-plan
description: Read-only install test (runs on every platform, no shell grant) - the agent must find the consult skill and produce the exact dry-run command and the files a real run would write, without executing anything.
tags: [smoke, install, readonly]
runs: 1
max_turns: 8
timeout_seconds: 180
allowed_tools:
  - Read
  - Glob
  - Grep
  - Skill
---

Without running anything: using the c3 plugin's skill, tell me the exact command you
would run for a DRY RUN consultation of task id `eval-smoke` with the ask "Is the bridge
installed correctly?" on this machine, and list the files a REAL (non-dry) structured
consultation of that task would create or update, with their paths relative to the
repository root. Do not execute the command, do not create files, and do not create or set
any API key.
