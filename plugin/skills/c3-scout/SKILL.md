---
name: c3-scout
description: Explore an EXTERNAL directory and generate a redacted directory-tree snapshot for context, through the `c3` binary. Use when you need to understand the architecture of a linked or external project without pulling its full source.
argument-hint: <path-to-external-repo> [--depth 0-9]
allowed-tools: Bash(c3:*), Read, Glob, Grep
disable-model-invocation: false
---

# Scout (c3)

The C3 port of eck's `eck-scout`: a thin layer over the `c3` binary. `c3 scout` takes a
snapshot of **another** directory (it may live outside this repository) and writes it into
this repository's `.eck/scouts/scout_tree_<dirname>_<ts>.md`, through the same discovery
(gitignore, `.eckignore`, the hard-ignore and never-commit filters) and the same secret
redactor the normal snapshot uses. A path **inside** this repository is refused — use
`c3-fetch` for files of the current repo.

Run it from this repository:

```bash
c3 scout <path-to-external-repo> --depth <N>
```

- `--depth` is 0-9 and defaults to `0`: `0` tree only, `5` skeleton (signatures), `9` full
  source. Start at `0` to map the shape, then raise the depth or fetch specific files.
- The scout file opens with a header that says how to get more detail (run `c3 fetch
  <glob>` from that repository, or re-scout at a higher depth).

`c3 scout --help` is the authoritative current flag list.
