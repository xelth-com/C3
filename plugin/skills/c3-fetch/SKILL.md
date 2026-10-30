---
name: c3-fetch
description: Fetch the full, redacted contents of specific source files of THIS repository by glob, through the `c3` binary. Use after a scout, or whenever you need the exact implementation of files selected by a pattern gathered into one document.
argument-hint: <glob>... (e.g. "**/api.rs" "crates/c3/src/pack/*.rs")
allowed-tools: Bash(c3:*), Read, Glob, Grep
disable-model-invocation: false
---

# Fetch (c3)

The C3 port of eck's `eck-fetch`: a thin layer over the `c3` binary. `c3 fetch` collects the
full contents of the files of **this** repository that match the given globs into
`.eck/scouts/scout_data_<ts>.md`, through the same discovery (gitignore, `.eckignore`, the
hard-ignore and never-commit filters) and the same secret redactor the snapshot uses — so
the gathered document is safe to carry into a reviewer's context.

Run it from this repository:

```bash
c3 fetch "<glob>" ["<glob>" ...]
```

- Globs are repo-relative and POSIX (`/`). Pass several to union their matches.
- Each included file is written under a `--- File: <path> ---` marker; secrets are replaced
  with a `[REDACTED:<kind>]` marker.

`c3 fetch --help` is the authoritative current flag list.
