---
type: llm
weight: 1
focus: last_message
---

PASS if the final message lists at least the built-in `openai` provider with a verdict
(available / unavailable / unknown) and the reason c3 printed (for example
"Logged in using ChatGPT", "missing: env X not set", "codex CLI not found"), lists every
other provider c3 printed the same way, and says whether a roster exists and what it would
pick. A machine with no usable reviewer is still a PASS when reported as such.

FAIL if a verdict is invented, a provider printed by c3 is dropped, the message claims a
network check happened, or it mentions creating or setting an API key.
