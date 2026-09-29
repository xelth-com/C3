# openrouter-live — the first live run of the `http` engine (2026-09-29)

Purpose: prove the API path end to end against a real concentrator, and try two nearly free models
the maintainer asked about. One question to both: a security review of
`crates/c3-core/src/roster_ext.rs` (the key-to-host binding), brief in `brief.md`.

Run from the clean worktree at 174bf48 with a scratch copy of the roster (the two reviewers under
`ext.c3.reviewers`); the key came from the process environment only and is in none of the files
(checked against the real value).

## Consultations

| n | reviewer | result | real wall | tokens in / out | mark |
|---|---|---|---|---|---|
| 1 | `openrouter :: stealth/space-bunny-alpha [http]` | usable, REJECT, 2 findings | about 100 s | 12 564 / 12 884 | yes |
| 2 | `openrouter :: nvidia/nemotron-3-ultra-550b-a55b:free [http]` | failed: upstream overloaded | 11 s | - | - |
| 3 | the same, second attempt | usable, HOLD, 3 findings | 450 s | 14 470 / 8 808 (5 701 reasoning) | partly |

Both replies were recorded as `structured: INVALID` (see D2); the findings below are taken from the
replies by hand.

## Findings and decisions

| id | from | severity | claim | decision |
|---|---|---|---|---|
| L1 | Space Bunny F1 | blocker | any `C3_KEY_*` variable may be sent to any https host: a manipulated roster entry or flag redirects a key created for one endpoint to another | accepted - D5 |
| L2 | Space Bunny F2 | major | a key value pasted into `key_env` is echoed by the refusal | accepted - D6 |
| L3 | Nemotron F1-F3 | major / minor | a field of the wrong type (`headers`, a header value, `base_url`) is echoed whole by `compact()` | accepted - D6 |
| L4 | coordinator | major | `wall_seconds` stops at the response headers (1.9 s and 0.8 s recorded for runs of 100 s and 450 s) | D1 |
| L5 | coordinator | major | an almost valid reply is discarded: `evidence` as one object instead of an array; raw line breaks inside JSON strings | D2 |
| L6 | coordinator | minor | the summary and the handoff name an events file that is never written | D3 |
| L7 | coordinator | minor | `Upstream error ... overloaded` is classed `unknown` instead of `unavailable` | D4 |
| L8 | coordinator | major | a custom header may carry a credential in the roster in clear text (`X-Api-Key`) | D6 |

Verified as holding (Space Bunny, with line references; Nemotron agrees): the known-key binding is
not defeated by case, a trailing dot, a port, userinfo, percent encoding, backslashes, IDNA
look-alikes or a suffix without the dot boundary; CR and LF cannot enter a header.

- D1. The clock stops after the body is read.
- D2. A deterministic local normaliser runs after a failed strict parse, for engines without an
  enforced output schema: fence, outermost object, control characters inside strings, a single
  object where an array is required; nothing is invented; what was done is recorded.
- D3. The http engine writes its events file: request and response metadata, header names only.
- D4. Provider errors are classed by the code in the body, then by the HTTP status.
- D5. The name of a custom key carries its host: `C3_KEY_` + the host in upper case, `.` as `_`,
  `-` as `__`; exact host only. The roster and the flags are writable by an agent, the variable
  name is what the user created by hand.
- D6. No refusal contains bytes of a value; a header whose name suggests a credential is refused.

## The two models as reviewers

- **Space Bunny Alpha** - the better review by a clear margin: it found the design-level hole (L1)
  that the coordinator's own review had missed, separated what it verified from what it could not
  see, and gave exact line references. One schema deviation. Free while it is in alpha.
- **Nemotron 3 Ultra (free)** - three valid findings of one root cause, none of them the main hole;
  it stated that `key_env` leaks only a name, which L2 contradicts. The free queue cost 450 s and
  one overloaded attempt. Usable as a second opinion where time does not matter.

Data policy, for the record: a stealth alpha model and a `:free` endpoint may log prompts and use
them for training. What was sent is the sanitised pack of this repository's own source
(`handoffs/0N-http-reply.pack.md`); no secret was in it.
