# The `http` engine (M7b, API path)

The `http` engine (DESIGN §4 "API path", state.md D4/D8) sends **one OpenAI-compatible
`chat/completions` request** built from a retained reviewer pack, for OpenRouter or any
endpoint that speaks the same wire shape. It is the API alternative to the subscription CLIs
(codex / agy / muse) for people who will not juggle subscriptions and consult rarely.

Module: `crates/c3/src/http_engine/mod.rs`. It implements `c3_core::engine::Engine`. The
dispatch into `c3 consult` (engine selection, roster wiring, ledger placement) is a **separate
step** owned by the orchestrator; this document is the contract that step wires against.

## Invariants this engine upholds

- **No tools.** The reviewer receives only the sanitized pack (DESIGN §3 invariant 2). The pack
  is the user message; the reply schema is the system message.
- **Key from the environment only** (invariant 4). The credential is read from `key_env` at run
  time, sent only in the `Authorization: Bearer` header to the provider, and **never printed,
  stored, committed, or transmitted elsewhere**. It is not held in `HttpConfig`, not in the
  `RequestPlan` (whose `Display` shows `Authorization: Bearer [REDACTED]`), and not in the pack
  or sidecar. Every error string passes through `scrub()` = the shared `pack::redact::redact`
  pass (catches `sk-or-…`, bearer headers, JWTs, …) **plus** a literal replacement of this run's
  key value.
- **One sanitizer** (invariant 5, D8). The pack body is already redacted by
  `pack::reviewer::build`; error strings go through `scrub()`.
- **No redirects** (M7b-b S4). The agent is built with `.redirects(0)`; a 3xx is a
  `ProviderFailure` of class `unavailable` ("the endpoint answered with a redirect (not
  followed)") — a followed redirect would re-send the pack to another host.
- **Key-to-host binding** (M7b-b S1, in `c3_core::roster_ext`). A `key_env` may be sent only to
  the host it belongs to (a known key to its provider, or a `C3_KEY_*` variable to any https
  host); enforced by the roster parser and the CLI flags, so the config the engine receives is
  already bound. Reserved/malformed headers (S5) are refused at the source and skipped by the
  adapter as defence in depth.
- **Repo-relative POSIX paths** (invariant 9) in the sidecar and `provider_config`.

## Configuration — `HttpConfig`

| field            | default                             | meaning |
|------------------|-------------------------------------|---------|
| `base_url`       | `https://openrouter.ai/api/v1`      | API base; `/chat/completions` is appended |
| `model`          | (required)                          | model id sent verbatim, e.g. `openai/gpt-5` |
| `key_env`        | `OPENROUTER_API_KEY`                | env var the key is read from |
| `headers`        | `[]`                                | extra headers; OpenRouter's `HTTP-Referer` / `X-Title` are optional |
| `timeout`        | 180 s                               | connect + read timeout |
| `provider_label` | `openrouter`                        | lineage key + subscription guard |
| `json_object`    | `true`                              | send `response_format: {"type":"json_object"}` when the endpoint supports it |
| `repo_root`      | `None`                              | makes the pack path in `provider_config` repo-relative |

The runtime `HttpEngine` also holds the `ReviewerPack` (from `pack::reviewer::build`) and a
`handoff_stem` (the handoff path without extension); `.pack.md` / `.pack.json` are appended.

## Environment variables

- `OPENROUTER_API_KEY` (or whatever `key_env` names) — the API key. Read only; never emitted.
  `precheck()` refuses the launch when it is unset. `key_status()` reports only
  `env <X> set` / `env <X> not set` — never the value.

## Capabilities

`capabilities()` delegates to `c3_core::engine::capabilities(EngineKind::Http)`:
`threads:false, fork:false, resume:false`, schema transport `[OutputSchema, PromptOnly]` (used
as prompt-only + JSON mode), effort vocabulary `openai`, `sandbox:false` (never receives tools),
file prefix `http`.

## Request shape

- Method/URL: `POST <base_url>/chat/completions`.
- Headers: `content-type: application/json`, `authorization: Bearer <key>`, then any `headers`.
- Body (primary turn):
  ```json
  {
    "model": "<model>",
    "messages": [
      { "role": "system",  "content": "<FINAL_OUTPUT_CONTRACT + reply-schema block>" },
      { "role": "user",    "content": "<sanitized reviewer pack>" }
    ],
    "response_format": { "type": "json_object" },   // when json_object
    "reasoning": { "effort": "<effort>" }           // when request.effort is set
  }
  ```
  The system message is `pack::reviewer::system_prompt()`.
- Body (continuation = **replay**): exactly **three** messages — the pack is re-sent as `user`,
  the prior assistant reply as `assistant`, and the new prompt as `user`. No separate system
  message is needed because the pack already ends with the reply schema. Triggered by
  `TurnRequest.continuation == Continuation::Replay { pack_hash, prior_reply }`. A retry reuses
  the captured inputs without a redraw.

## Response mapping → `AttemptOutcome`

| provider result                    | outcome |
|------------------------------------|---------|
| 200, `choices[0].message.content` is one v1 JSON object (fenced tolerated) | `Completed(Reply { structured: Some, .. })` |
| 200, content is prose              | `Completed(Reply { structured: None, .. })` (orchestrator repairs) |
| 200 with `{"error":…}` envelope    | `ProviderFailure` (class from message) |
| 401 / 403                          | `ProviderFailure { class: "auth", code }` |
| 429                                | `ProviderFailure { class: "quota", retry_after }` (from `Retry-After`) |
| 5xx                                | `ProviderFailure { class: "transport" }` |
| transport timeout                  | `TimedOut { partial: None, survivors: [], conversation: Candidate }` |
| connection refused / other transport | `ProviderFailure { class: "transport" }` |

Usage maps `prompt_tokens → input_tokens`, `completion_tokens → output_tokens`,
`total_tokens → total_tokens`, `completion_tokens_details.reasoning_tokens →
reasoning_output_tokens`. Wall time is measured and rounded to 1 decimal. The conversation is a
fresh **client-owned** `ConversationId` tagged `ConversationTrust::Candidate` (`http` has no
native thread; `resume_supported: false`). `Reply.events_path` points at the `.pack.json`
sidecar (the engine has no event stream; the pack is its record).

## Pack / sidecar contract (retained BEFORE the request)

Before the POST, the engine writes:

- `<stem>.pack.md` — the exact sanitized pack sent (`ReviewerPack.content`).
- `<stem>.pack.json` — the `pack::reviewer::build` sidecar (content hashes, path map, coverage,
  recipe) **extended** by `pack::reviewer::sidecar_with_request(..)` with:
  ```json
  "request": { "url": "...", "model": "...", "response_format": "json_object" | "none",
               "prompt_sha256": "<sha256 of the serialized messages>" }
  ```

Writing before the request means a later reader knows what the reviewer saw even when the
request fails. A `read-code` finding's `reference` names the pack path and hash.

## The billing guard (`precheck`)

`precheck()` refuses (`EngineError::Precheck`) in two cases, and never emits the key:

1. **Subscription engine on the API path.** If `provider_label` (case-insensitive) is one of
   `SUBSCRIPTION_PROVIDERS = ["codex", "chatgpt", "muse", "agy", "antigravity"]`, the launch is
   refused: sending an API key to a provider that has a signed-in subscription CLI would bill
   per token where the subscription is already paid (the muse rule, generalized). The API
   concentrators/labs (`openrouter`, `openai`, `anthropic`, `google`, …) are deliberately absent.
2. **No key.** When `key_env` is unset/empty, the launch is refused (env-only rule).

## Wiring the orchestrator must do (DONE — M7b-b)

This section was the work list for making `http` a first-class reviewer in `c3 consult` and
the panel; it is now implemented (see `docs/port/m7-status.md`). The seam is `crate::consult::http`
(the seat's run path and dry-run render) plus `crate::roster_ext` (the `ext.c3.reviewers` parser),
called from a few lines in `orchestrate.rs`. The list below records the original contract.



1. **Engine selection:** `--engine http` selects `HttpEngine` for a seat. Build `HttpConfig`
   from the roster entry's `ext.c3` (see below); build the `ReviewerPack` via
   `pack::reviewer::build`; set `handoff_stem` to the seat's handoff path without extension.
2. **Roster entry shape** (roster_version 1, wave 26 `ext` object):
   ```json
   {
     "provider": "openrouter",
     "model": "openai/gpt-5",
     "engine": "http",
     "ext": { "c3": { "base_url": "https://openrouter.ai/api/v1",
                       "key_env": "OPENROUTER_API_KEY",
                       "json_object": true,
                       "headers": { "HTTP-Referer": "https://xelth.com", "X-Title": "c3" } } }
   }
   ```
   Until wave 26 lands in the validator, the same data lives in the sidecar (state.md §5).
3. **Refusals:** call `precheck()` before the run; surface `EngineError::Precheck` as the
   seat's refusal reason (subscription-guard or missing key). The lineage is
   `provider :: model :: http`; never resume/fork across lineages (`resume_supported: false`
   means the ledger records no resumable conversation).
4. **File prefix:** `http` — handoffs are `handoffs/<NN>-http-<slug>[-<provider>].md`, and the
   pack files are `<same stem>.pack.md` / `.pack.json`.
5. **Ledger fields:** place `HttpAttempt.provider_config` on `reviewer.provider_config`:
   ```json
   { "engine": "http", "base_url": "...", "model": "...",
     "pack": "<repo-relative pack.md path>", "pack_sha256": "<sha256 of pack content>" }
   ```
   Record `usage`, `wall_seconds`, `structured`, the finding counts, and (on failure) the
   `ProviderFailure`. `resume_supported` is `false`. The `.pack.md` / `.pack.json` paths are on
   `HttpAttempt` for the handoff record.

## Entry points

- `HttpEngine::attempt(&turn) -> HttpAttempt` — full fidelity: retains the pack, POSTs, returns
  `{ outcome, pack_md, pack_json, provider_config }`.
- `Engine::run` / `Engine::continue_turn` — return only the `AttemptOutcome` (continuation is
  replay, decided by `turn.continuation`).
- `HttpEngine::request_plan(&turn) -> RequestPlan` — a redactable wire view for logging.
