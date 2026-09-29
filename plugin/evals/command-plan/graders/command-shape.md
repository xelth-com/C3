---
type: regex
weight: 2
target: last_message
match: contains
pattern: "c3 consult[^\\n]*--task\\s+eval-smoke[^\\n]*--dry-run|c3 consult[^\\n]*--dry-run[^\\n]*--task\\s+eval-smoke"
flags: i
---

The final message must contain the `c3 consult` command with `--task eval-smoke` and `--dry-run`.
