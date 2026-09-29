---
type: llm
weight: 1
focus: last_message
---

PASS if the final message reports, from the bridge's own dry-run output: the fixed first line
("DRY RUN - nothing was executed and no file was written."), a preflight verdict for the
resolved reviewer (available / unavailable / unknown, or skipped, with its reason), the
reviewer line (provider and model, or that the identity is unresolved), and states that nothing
was written. A preflight of "unavailable" or "unknown" is still a PASS when it is reported
honestly - the machine may have no reviewer credentials.

FAIL if the message invents output the bridge did not print, claims a real consultation ran,
mentions creating or setting an API key, or omits the preflight verdict.
