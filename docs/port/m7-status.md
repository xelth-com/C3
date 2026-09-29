# M7b-b status — the `http` engine wired into `c3 consult` and the panel

The `http` engine adapter (`crate::http_engine`, `docs/port/http-engine.md`) is now a
first-class reviewer: `c3 consult --engine http ...`, an `ext.c3.reviewers` roster entry,
a panel seat, a `c3 providers` row and a dedicated dry-run block. It is the API path for
someone with no subscriptions who consults rarely (OpenRouter by default).

## What runs

- **Roster placement (`crate::roster_ext`).** The roster is shared with the PowerShell
  plugin, whose validator refuses any engine other than `codex`/`agy`/`muse`. So `http`
  reviewers live under the top-level `ext.c3.reviewers` array the plugin validates as an
  object and ignores. `crate::roster_ext::parse_ext_reviewers` validates them with the same
  rules and wording style as an ordinary entry (clean provider/model, `lab`, slug `roles`),
  plus the API additions: `base_url` is `https://`, `key_env` is a valid environment-variable
  name, header names/values carry no CR/LF and none is `Authorization`. The C3 roster loader
  (`providers::read_reviewer_roster`) appends them to `roster.entries` as synthesized `http`
  entries AFTER the plugin's entries (positions continue), so the panel and `c3 providers`
  (which iterate `roster.entries`) see them; the full request config is kept on
  `Roster::http_reviewers` and looked up by position/identity.
- **Direct run.** `c3 consult --engine http --provider <label> --model <id>` works with the
  OpenRouter defaults; `--base-url <https url>` and `--key-env <NAME>` override. The key is
  read from the environment only — never a flag, a config value, a roster field, a log, an
  error, a ledger field or a dry-run line.
- **Billing guard (`consult::http::billing_precheck`).** Refuses a subscription-engine label
  (`codex`, `chatgpt`, `muse`, `agy`, `antigravity`) outright, a lab label (`openai`,
  `gemini`, `google`, `meta`) unless the roster entry says `"api_billing": "accepted"`, and a
  launch with no key set (naming the variable, never a value).
- **Pack only.** The seat builds a `pack::reviewer::build` reviewer pack from the brief and
  the bound artifacts (the focus files), retains it and its sidecar BEFORE the request, and
  sends one OpenAI-compatible request. `--mode fork`, and any resume/continuation across a
  non-existent thread, no-op through the existing engine gates (`http` has
  `resume:false`/`denial_retry:false`, and never verifies a thread).
- **Records.** File prefix `http`; `reviewer.provider_config = {engine, base_url, model, pack,
  pack_sha256}` (built by the adapter, applied to the identity in `finish`); endpoint health
  keyed by the `provider :: model :: http` fingerprint. The tree check treats `http` as
  write-disabled (it never touches the machine): a tree change during the run WARNS, not FAILS.
- **Dry run (`consult::http::render_dry_run`).** Prints the endpoint, model,
  `key: env <NAME> set|not set`, the pack size (tokens and files) and the request plan with
  `Authorization: Bearer [REDACTED]`. Makes no network call and writes nothing (the pack is
  built in memory only).
- **caps-v1.** `engine:http` is declared with the OpenAI effort vocabulary
  (low/medium/high/xhigh) and prompt-only transport (the schema travels in the pack; JSON mode
  is requested via `response_format`).

## The wiring seam (a few lines in `orchestrate.rs`)

- `dispatch` routes `http` to `consult::http::render_dry_run` (dry) or the shared `run_live`
  (real), exempting it from the launcher-not-found refusal.
- `run_primary_turn` gets an `"http"` arm that calls `consult::http::run_seat` and returns the
  outcome plus the provider_config (carried on `EngineDetail::http_provider_config`, applied in
  `finish`). Ingest, ledger commit, handoff render and summary are the shared engine path.
- `build_context` handles `LaunchPlan::Http` (no subprocess argv) and refuses `--key-env` /
  `--base-url` on a non-http engine, a non-`https://` base URL, or an http run with no provider.
- `providers::preflight` gets an `http_credential` branch (available when `key_env` is set);
  `build_engine_row` shows the endpoint as the API base and the credential as `env <NAME> set`.
- The panel's up-front launcher preflight exempts `http` (it has no CLI launcher).

## Verification

- `cargo test --no-default-features`: c3 lib 245, `http_engine` integration 9 (incl. the
  seeded-secret grep of every written file and outcome string), `c3-core` 69 (incl.
  `roster_ext` — every refusal). `cargo fmt --all -- --check` and
  `cargo clippy -p c3 -p c3-core --no-default-features --all-targets -- -D warnings` clean.
- End-to-end (worktree binary): a direct-run http dry-run, the lab-label billing refusal, a
  `c3 providers` http row (`ok: env OPENROUTER_API_KEY set`), and a 2-seat panel dry run with a
  codex seat and an http seat (both `planned`, the http seat rendering its own redacted request
  plan).

## Security hardening (S1–S6)

- **S1 key-to-host binding (`roster_ext::check_key_host`).** c3 sends an environment variable
  only to the host it belongs to. Known keys are bound (OPENROUTER_API_KEY→openrouter.ai,
  OPENAI_API_KEY→api.openai.com, ANTHROPIC/GEMINI/GOOGLE/MISTRAL/DEEPSEEK/GROQ/TOGETHER/XAI to
  their hosts), matched as an exact host or a subdomain (never a substring). Any other endpoint
  needs a variable the user made for c3, named `C3_KEY_<X>`. Everything else is refused, naming
  the variable and the rule, never a value. Used by BOTH the roster parser and the CLI flags.
- **S2 base URL parsing (`roster_ext::parse_base_url`, via the `url` crate).** Scheme exactly
  `https`, a non-empty host, no userinfo/query/fragment, an optional port, a path allowed, no
  control characters, ≤2048 bytes. The host for S1 is the parsed host, lowercased.
- **S3.** `--key-env` is validated as an env-var name and by S1; `--base-url` by S2 and S1;
  both in the pre-launch refusal style.
- **S4 no redirects (`http_engine`).** The agent is built with `.redirects(0)`; a 3xx is a
  failure of class `unavailable` ("the endpoint answered with a redirect (not followed)") — a
  redirect would re-send the pack to another host.
- **S5 reserved headers.** The roster validator refuses (case-insensitively) `authorization`,
  `proxy-authorization`, `cookie`, `host`, `content-length`, `content-type`,
  `transfer-encoding`, and any name that is not an RFC 7230 token. The adapter additionally
  skips any such header if one ever reaches it (defence in depth).
- **S6 pack periphery budget.** Default 12000 tokens for an http seat; roster `pack_tokens`
  (0..=200000) per `ext.c3.reviewers` entry; CLI `--pack-budget <n>` (same range) wins. `0`
  keeps the brief+artifacts-only pack. The dry run prints the pack size and one line
  `billing : per token - this request sends about <N> tokens` (pack + reply-schema estimate).

## First live run — six defects fixed

The engine ran live for the first time against two OpenRouter models and showed six defects, now
fixed (see `docs/port/http-engine.md` for the mechanics):

1. **Wall time (item 1).** The clock stopped after the response headers, not the body, so a
   100 s / 450 s run was logged as 1.9 s / 0.8 s. `http_engine::post` now stops the clock after
   the body has been read on every path.
2. **Near-valid replies discarded (item 2).** Both live replies were real structured reviews
   recorded as `structured: INVALID` (one had `findings[].evidence` as a single object, the other
   raw line breaks inside JSON strings). A deterministic, local normaliser
   (`consult::ingest::normalise_reply`) runs ONLY after the strict parse fails and only for the
   http path: fence strip, outermost object, control-char escaping inside strings, single-value →
   array wrapping, `schema_version` default/coerce; it never invents content. A repaired reply
   records a `reply normalised: <list>` warning in the ledger and the summary. Regression fixtures:
   `crates/c3/tests/fixtures/http/{01,03}-http-reply.reply.json`.
   - **Second review round (N1-N3), from a live review of the normaliser itself
     (`04-http-reply.reply.json`, recorded INVALID: duplicate `schema_version`).**
     - **N1 duplicate keys.** A duplicate-aware `serde` walk runs over the raw object text before any
       lossy `Value` conversion (which would keep the last silently, e.g. turning
       `"verdict":"REJECT","verdict":"ACCEPT"` into a valid ACCEPT): identical duplicates keep one
       with a note, different values stay INVALID naming the key (never the values).
     - **N2 outermost object.** Instead of the first balanced `{...}`, the normaliser collects every
       top-level object (tracking strings, skipping code fences/spans) and takes the single one that
       parses with `verdict` + `findings`; none or several → INVALID with `no single reply object
       found (<n> candidates)`. Fixes a sample object in the prose masquerading as the reply.
     - **N3 notes / reply-of-record / reason.** Notes are non-empty iff the text changed; the
       `normalised` event is written only with ≥1 note; on success the repaired JSON becomes the
       reply-of-record so the ledger records it structured; on failure the summary keeps the strict
       error and appends `; normaliser: <reason>`. Idempotence and linear cost are asserted by test.
3. **Named-but-unwritten events file (item 3).** The summary and handoff named
   `handoffs/<NN>-http-reply.events.jsonl`, never written. The engine now writes it: `request` /
   `response` / `normalised` / `error` JSON lines, header NAMES only, never the key or the body.
4. **Body error class (item 4).** A provider error in the body (`{"error":{"code":…}}`, sometimes
   under HTTP 200) was classed `unknown`. Classification is now by the numeric code (body, else
   HTTP status) then the message: 401/403 → auth, 402 → quota, 429 → burst (+retry_after), 408/5xx
   /overloaded/timeout → unavailable, 400 context-length → capability.
5. **S1 `C3_KEY_` escape hatch (item 5, security).** A `C3_KEY_*` variable could be sent to ANY
   https host. Now a custom key is bound by its NAME to exactly one host (`C3_KEY_` + host
   upper-cased, `.`→`_`, `-`→`__`; injective; an IP literal cannot use one), enforced in
   `roster_ext::check_key_host`. The refusal names the variable to create for the host in hand.
6. **Refusals echoing secrets (item 6, security).** Refusals echoed field values (`compact(value)`,
   the `key_env` name) that could be pasted keys. No refusal now carries a `key_env`/`base_url`
   /`headers`/header value or a wrong-typed field's bytes — it reports the JSON type; a `key_env` is
   named only when known or a well-formed `C3_KEY_` name, with a hint (not an echo) when it looks
   like a key value. Header NAMES that suggest a credential are refused. A canary test asserts no
   marker fed into any field leaks.

## Secondary turns — format repair and timeout retry (STEP 2)

The http seat now runs its own secondary turn in-process (the orchestrator's thread-based
`run_format_repair` / `run_timeout_continuation` remain no-ops for `http`, which has no thread):

- **Format repair (replay).** When the reply is still not structured after the normaliser and the
  prose gate (`consult::ingest`) says it is substantive, ONE `Continuation::Replay` request is sent
  (system/pack + the prior reply + the plugin's convert-only prompt, shared via
  `orchestrate::format_repair_prompt`). The first reply is kept byte for byte as `<stem>.original.md`;
  the normaliser runs on the repaired reply too; on success the repaired JSON becomes the
  reply-of-record. The ledger records it exactly as the codex path does (`format_retry` fields,
  `engine_turns: 2`), and the events file gains a second `request`/`response` pair tagged
  `"turn":"format-repair"`. A failed repair keeps the prose, as before.
- **Timeout retry.** The API returns nothing on a timeout, so a "continuation" is a RETRY of the
  same request, at most once, only for an `unavailable` failure (a request timeout, a 5xx, or an
  overloaded/unavailable answer), after a pause of the provider's `retry_after` (else 20 s, never
  more than 120 s). `auth`, `quota` and `burst` are not retried. `--no-continue` (continue_sec 0)
  suppresses it. Recorded as `engine_turns: 2` with a `retried once after <class>` warning; the
  events get a second pair tagged `"turn":"retry"`.
- **Billing.** Both send the pack a second time; the dry-run billing line says
  `... ; a format repair or a retry sends them once more`. The key-host binding, redirect refusal
  and scrubbed errors apply to the second request unchanged (it goes through the same `post`).

The turn-driving is split into `consult::http::drive_seat_turns` so it is unit-tested against the
in-process fake server without a full orchestrator context.

## Known limitations / follow-ups

- (none open for the http engine at this milestone)
