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

## Second run (2026-09-29, the release binary with the fixes, 4e8d1a8)

| n | reviewer | question | result | wall recorded / real | mark |
|---|---|---|---|---|---|
| 4 | `openrouter :: stealth/space-bunny-alpha [http]` | the reply normaliser (`brief2.md`) | usable, REJECT, 2 findings | 163.3 s / 167 s | yes |

Confirmed live: D1 (the wall clock covers the body), D3 (the events file: request, response,
normalised; header names only); the key is in none of the files.

Not yet: the reply was again recorded as `structured: INVALID` - the model wrote the key
`schema_version` twice, both `"1"` - and the normaliser changed nothing.

| id | from | severity | claim | decision |
|---|---|---|---|---|
| L9 | Space Bunny F1 | major | the first balanced `{...}` wins, so a sample object before the real reply replaces it | accepted - D7 |
| L10 | Space Bunny F2 | major | duplicate keys collapse silently, the last one wins: `REJECT` then `ACCEPT` becomes a valid ACCEPT | accepted - D8 |
| L11 | coordinator | minor | a duplicate key with identical values (the live case) should be repaired, not refused | D8 |

- D7. Every top-level object outside code fences is a candidate; it counts only with the keys
  `verdict` and `findings`; exactly one candidate is taken, otherwise the reply stays invalid.
- D8. A duplicate-aware pass runs before any conversion: identical values are merged with a note,
  different values keep the reply invalid; the reason names the key, never the values.

## Which model is Space Bunny Alpha: a tokenizer fingerprint

A stealth model hides its name, not its tokenizer: the provider reports `prompt_tokens`, and for
the real tokenizer `prompt_tokens - T(text)` is the same constant for every text (the chat framing
plus the provider's own hidden system text). Computed locally from public tokenizer files
(`fingerprint/`); nothing but the five requests below was sent.

| measurement | text | prompt_tokens | MiniMax M2/M2.5 tokenizer | left over |
|---|---|---|---|---|
| run 1 | our system message + pack 1 | 12 564 | 308 + 12 106 | 150 |
| run 4 | our system message + pack 4 | 13 126 | 308 + 12 668 | 150 |
| probe A | 2 000 characters of pack 1 | 621 | 465 | 156 |
| probe B | 4 500 characters of pack 4 | 1 318 | 1 162 | 156 |
| probe C | `ping` | 165 | 9 | 156 |

The same constant five times out of five (150 with a system message of ours, 156 without). Every
other tokenizer tried leaves a different remainder in each measurement: OpenAI `o200k` (+176, +155,
+156 on the probes), GLM 4.7-5.3 (+143, +147), DeepSeek V4 and Step 3.x (+145, +99), Cohere
(+179, +186), Llama 4 (+172, +178), and further off Kimi K2-K3, Qwen 3.8, MiMo / Hunyuan, Mistral /
Nemotron, Gemma 3-4, ERNIE 4.5, the old Claude and Grok-1 files.

Control: the same arithmetic on Nemotron's run gives +16 with NVIDIA's own Nemotron tokenizer
(the same file as Mistral's), so the method reads the right thing.

Conclusion: Space Bunny Alpha tokenizes with the MiniMax M2 / M2.5 tokenizer, so it is most likely
the next MiniMax text model. Tokenizer files are shared between laboratories now and then (Step
uses DeepSeek's, Phi-4 uses `o200k`, Hunyuan and MiMo share one), so strictly this identifies the
tokenizer family, not the owner. About 150 hidden tokens are the provider's own system text.

## Third run (2026-09-29, the binary of 3503b51): the whole path works

| n | reviewer | question | result | wall recorded / real | mark |
|---|---|---|---|---|---|
| 5 | `openrouter :: stealth/space-bunny-alpha [http]` | the second request of the http engine (`brief3.md`) | usable, HOLD, F05-1..F05-2 recorded | 155.3 s / 178 s | yes |

For the first time a reply of an API reviewer became STRUCTURED: the normaliser wrapped `evidence`
(one object, twice) into arrays, the reviewer's own text is kept as `05-http-reply.original.json`,
and the two findings are in `findings.json` under their ids.

| id | severity | claim | decision |
|---|---|---|---|
| F05-1 | major | the result of writing the first reply (`.original.md`) is discarded, so a write error is hidden while the record names the file | accepted: the write error is handled before the repair request is sent |
| F05-2 | major | after a failed format repair the first prose reply is still reported as a usable reply | rejected: by design, as in the plugin - prose is a usable reply without structured findings; the ledger says `structured: false` and records the repair as not succeeded |

Verified by the reviewer as holding: `drive_seat_turns` sends at most two requests for one
consultation; no retry after `auth`; `--no-continue` suppresses the retry.
