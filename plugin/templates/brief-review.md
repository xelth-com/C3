<!-- Serves -Purpose checkpoint | core-contract | acceptance | diff-review. -->
<!-- The script never writes briefs - you fill this by hand and pass it via -Brief. -->

# Handoff <NN> - Claude: <slug>

Date: <date>. Base commit: `<sha>`<+ uncommitted, if any>.

## Question

<The decision or judgement requested, in one or two sentences - e.g. "Is the recovery
path safe to build on?" or "Can this be accepted as done?">

## Delta since the last review

Follows: `<prior handoff, e.g. handoffs/04-codex-checkpoint.md>` (or "first review of this task").

- <What changed since that review - files, behaviour, decisions. Not a full history.>
- <...>

## CURRENT invariants claimed

<History is not an authoritative current-state record. State what is true of the code
NOW, not only what changed since last time - a reviewer working from the delta alone
will miss anything that was already true and still matters.>

- <Invariant 1, e.g. "every write path holds the lock before allocating a finding id">
- <Invariant 2>

## Changed files

Base commit `<sha>`, fingerprint `<tree_sha256 first 12 hex, or "not computed">`.

| File | Change |
|---|---|
| `<path>` | <one line> |

## Open findings

From `codex-findings.ps1 -Task <task> -List`:

- **<F04-1>** <status> - `<location>` - <claim, first line>
- <...> / _(none open)_

## Requested checks run

<From the previous reply's `## Requested checks` section (RC1..RCn), if any - fill one
row per check before writing this brief; omit the table entirely when the previous
reply had none.>

| check (handoff-RCn) | command | revision (base + tree_sha256 first 12) | exit status | log path or hash | observation | state (completed/failed/skipped) |
|---|---|---|---|---|---|---|
| `<NN-RC1>` | `<command>` | `<sha> + <tree_sha256[:12]>` | `<0>` | `<path or hash>` | <what it showed> | completed |

## Evidence

- `<path:line>` - <what it shows>
- <log or test output path> - <what it shows>
- <...>

## Small critical executables

<Paste verbatim, under ~100 lines each - a boot hook, an install script, anything where
reading the exact bytes matters more than a description. Everything else stays out of
this brief: exact path + revision only.>

```
<paste>
```

## Questions

- **Q1.** <...>
- **Q2.** <...>

Answer by number. Keep it under <N> words.
