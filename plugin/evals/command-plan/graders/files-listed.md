---
type: llm
weight: 1
focus: last_message
---

PASS if the final message (a) shows one `c3 consult` command with `--task eval-smoke`, `--dry-run`
and a `--prompt` (or a `--brief`) for the ask, and (b) lists, for a real structured run of that
task, at least: `.collab/eval-smoke/sessions.json` (the ledger), a reply file
`handoffs/NN-codex-<slug>.md` under `.collab/eval-smoke/`, its raw `.reply.json`, the events
`.events.jsonl`, and `.collab/eval-smoke/findings.json` when findings exist; mentioning the
runtime files `.consult.lock` / `.consult.pending.json` is a bonus, not required.

FAIL if the command lacks `--dry-run` or `--task eval-smoke`, if the message claims to have run
it, if files are invented that `c3` does not write, or if it mentions creating or setting an API
key.
