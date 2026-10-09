# Wave 5 of the compatibility track: the last known gaps before 0.2.0 (2026-10-09)

Branch `wave5-leftovers`. The specification is the plugin at `v0.6.1`: `codex-consult.ps1`
(`-BriefPrefix`, the coordinator's resolution at the start of a run, the dry-run block),
`codex-consult-common.ps1` (`ConvertFrom-ReviewerMatcher`, `Get-IdentityStringProblem`,
`Resolve-CoordinatorIdentity`, `Invoke-TelemetryRequest`, `Invoke-TelemetrySend`,
`Enter-TelemetryFlushLock`, `Get-TelemetryFlushLockOwner`, `Invoke-TelemetryFlush`),
`codex-telemetry.ps1` (`-Flush`, `-Status`) and the README sections "Telemetry (on by default)"
and the options table. The oracles are harness-host PREFIX / REFUSE / WARN and the harness-telemetry
sender rows, through the shim (`harness-shim.md`).

## 1 - `--brief-prefix` (the plugin's wave 27c, D6)

- `c3 consult --brief-prefix <slug>` (`Options.brief_prefix`); empty: `CODEX_CONSULT_BRIEF_PREFIX`,
  else `claude` (`args::resolve_brief_prefix`, called from `args::validate` - so a single run, the
  `-Detach` foreground, the detached background and a panel member all check it before anything
  is planned or written, the dry run too). Refusals, the plugin's texts (`Stop-WithError`, exit 1):
  - a reply prefix of the bridge: `the brief prefix '<p>' (<source>) is a reply prefix: the bridge
    names its replies handoffs/<NN>-<codex|agy|muse|claudecode|http>-<slug>.*; give the
    coordinator's briefs a prefix of their own (the default: claude); nothing was started.`
  - not a lowercase slug (`^[a-z][a-z0-9-]{0,31}$`): `the brief prefix '<p>' (<source>) must be a
    lowercase slug (a letter, then letters, digits or dashes; at most 32 characters); nothing was
    started.`
- The dry run prints, after `child env`, `brief prefix: <p> (<source>) - the coordinator's briefs
  are handoffs/<NN>-<p>-<slug>.md, this reply <NN>-<engine prefix>-<reply name>.*`.
- A panel hands its RESOLVED prefix to every member in the member spec (`brief_prefix`, as the
  plugin's `-PanelSpec`); the detached background gets the run's `--brief-prefix` in its args.
- The consult shim forwards `-BriefPrefix` as `--brief-prefix` (it had no such parameter: the
  harness's `-BriefPrefix codex` failed with "no parameter matches").
- **Decided differently:** the refused reply prefixes are EVERY engine's handoff prefix, C3's own
  `http` engine included (`lineage::ALL_ENGINE_NAMES`), so the list in the text reads
  `codex|agy|muse|claudecode|http` where the plugin's reads `codex|agy|muse|claudecode`. A brief
  named `NN-http-*` would read as an http reviewer's reply.

Tests: `args::tests::brief_prefix_resolution_and_refusals`; `c3-cli/tests/compat_wave5.rs`
`a_reply_prefix_or_a_non_slug_is_refused_as_the_brief_prefix` (every reply prefix, the variable,
a non-slug, a real run - nothing written, no test-mode line: host TESTLINE D12) and
`the_dry_run_names_the_brief_prefix_and_its_source` (the default line verbatim, the variable, the
flag over the variable with another reply name, a real run with its own prefix).

## 2 - REFUSE D3: the coordinator's character rule

The plugin parses `CODEX_CONSULT_COORDINATOR` with `ConvertFrom-ReviewerMatcher` - a grammar
WITHOUT a character class: `open::ai` (no blanks around `::`) is a provider LABEL - and then refuses
what `Get-IdentityStringProblem` finds in the provider and the model: empty, surrounding blanks, or
one of the matcher's and the seed's delimiters `::` `[` `]` `|` `,` `#` (`must not contain '::'`).
C3's shared grammar (`roster::parse_reviewer_matcher_grammar`, wave 2d) carried a label pattern
(`[A-Za-z0-9._-]+`, "is not a provider label") and a white-space check on the model that the plugin
has nowhere, so `open::ai` was refused with C3's own text and `openai :: gpt|5` was ACCEPTED (and
ran). Now:

- the grammar has no character rule (as `ConvertFrom-ReviewerMatcher`): `--require 'open ai'`
  says the plugin's `'open ai' matches no roster entry`;
- `roster::identity_string_problem` is `Get-IdentityStringProblem`, applied by
  `host::resolve_coordinator_match` after the `[1m]` strip, provider first, then the model -
  `CODEX_CONSULT_COORDINATOR='open::ai' cannot be used: the provider 'open::ai' must not contain
  '::' - give ...; nothing was started.`; interior blanks (`open ai :: gpt 5`) are accepted, as in
  the plugin;
- the refusal comes before anything is printed - the real run no longer reaches the test-mode
  line (the harness reads the FIRST line).

The coordinator's dry-run line and the D11 console line now follow `Format-CoordinatorText` /
`Format-CoordinatorId` exactly (found while checking WARN D3): the ` [<engine>]` suffix only for an
engine other than codex (C3 printed `openai :: gpt-5.1 [codex]`), `<provider> (model not named)`
(C3: "(every model of it)"), a `#n` with no seat shown as `#n (names no roster position here); host
...` (C3 printed the no-identity note and appended the `#n` after the source), and the D11 line
`coordinator: <resolved identity> (not in the roster - no reviewer can match it)` (C3 printed the
raw variable) - not for an unresolved `#n`, which has its own warning.

WARN D3: the rows that still fail expect `host codex` (the plugin's multi-host order) - not
applicable to C3, the single-host decision (`harness-shim.md`, "Not applicable to C3"); apart from
the host, their conditions hold (the dry run's warning line once, the preview's `warnings[]`, the
coordinator `{openai, gpt-5.1, codex, explicit}` after `lineage`, `child_env_scrubbed`
`[CODEX_SESSION_ID]` after `command` - checked by hand against the final binary).

Tests: `host::tests::matcher_refusals` (the four delimiters, interior blanks accepted),
`coordinator_position_and_roster_membership` (the full refusal text), `coordinator_text` (the
codex suffix, a model-less label with another engine, an unresolved `#n`, `format_coordinator_id`);
`compat_wave5.rs` `an_unparseable_coordinator_is_refused_with_the_plugins_wording` (the dry run's
and a real run's first line, nothing written, interior blanks run),
`the_coordinators_own_model_seated_is_a_warning_with_the_resolved_triple` (WARN D3's real run:
the ledger triple, the warning once, the console line; another model seats none) and
`the_coordinator_line_is_the_plugins` (fixes27c COORD D11/D12 through the binary).

## 3 - the telemetry sender's parity

C3's sender posted ONE batch of at most 100 events with a 3 s timeout and counted any 2xx as
delivered; a 4xx/5xx kept the spool. It now follows the plugin's `Invoke-TelemetryFlush` against
the intake as it is built (`telemetry/spool.rs`):

- **Delivered** only for a 2xx whose body is the intake's JSON object with `ok: true`
  (`PostAnswer::answer`); otherwise "HTTP <s> (<type>) is not the intake's JSON answer",
  "HTTP <s>: <error>" or "HTTP <s> without ok: true"; no answer: "no answer (...)" / "no answer
  within <n> s". No redirect is followed.
- **Every batch:** every fresh event in batches of at most 100, oldest first (the spool's order),
  until one is not delivered.
- **429** (`send_rate_ruled` = `Invoke-TelemetrySend`): a `Retry-After` (seconds, or an HTTP date)
  of at most 60 s that fits into what is left of the flush's deadline with a second to spare is
  waited for and the SAME body sent once more; a second 429 stops ("HTTP 429 again after its
  Retry-After"); a longer one ("Retry-After 120 s, more than 60 s: not retried now"), a missing one
  ("Retry-After not given") or one that does not fit stops at once. The batch stays queued.
- **400 `events[i]: <reason>`** (in the answer's `error`, else its text; `i` inside the batch):
  event i is DROPPED - one line `event queued <local time> refused: <reason>` in the report's and
  the record's `rejected[]` - and the rest resent; at most three such refusals per flush, the
  fourth stops it ("HTTP 400: <error> - a fourth refused event in this flush; the rest stays").
- **413:** the batch is halved (the smaller size holds for the rest of the flush); an event refused
  alone is dropped (`rejected`: `HTTP 413 (too large alone)`) - never resent forever.
- **403:** "HTTP 403 - the intake refuses this app (<why>); the spool is kept"; another 4xx:
  "<why> - the spool is kept"; a 5xx or no answer: "<why>". The flush stops; nothing is hammered.
- **The deadline:** one flush ends after 60 s in all (from its start, before the lock), one
  request after 8 s (connect 3 s); a request starts only while 1.5 s are left (1 s kept back for
  the rewrite); the snapshot read waits at most what is left; the rewrite of what was delivered is
  always attempted (at least 0.1 s). TEST HOOKS (test mode only, the plugin's names):
  `CODEX_CONSULT_TEST_TELEMETRY_FLUSH_MS`, `CODEX_CONSULT_TEST_TELEMETRY_REQUEST_MS`;
  `Spool::with_limits` for the Rust tests.
- **The record and the line:** `last-flush.json` `dropped` counts the refused events too and
  `rejected[]` names them; `result` is the plugin's line - `nothing to send`, `delivered <n>, kept
  <k>, dropped <d> (older than 7 days or unreadable[, or refused] | refused by the intake)`, `not
  delivered: <why> - delivered <n>, kept <k>, dropped <d>[, <r> refused by the intake]` (C3's
  `skipped - <why>` and its "discarded ... no closable C3 event" suffix stay). `c3 telemetry
  --flush` prints `codex-telemetry: <result>` and exits 1 when the send stopped (0 otherwise, 2
  when another sender holds the lock), as `codex-telemetry.ps1 -Flush`.
- **The lock's owner record:** the OS lock `flush.lock` stays THE sender lock (released by the OS
  when its holder dies - never stale, never taken over, so the plugin's 30 s ownerless rule and its
  take-over have nothing to do here). Right after taking it a sender writes `flush.owner.json`
  `{pid, start_time, start_ticks, token, since, host}` whole (temporary file + rename) and removes
  it, while it still carries its token, before the lock is released. A separate file because a
  file under `LockFileEx` cannot be read by another process on Windows. A sender that finds the
  lock busy says who holds it: `another flush is running: sender busy since <t> (pid <n> holds its
  lock, <s> s old)`, and once the record is 30 minutes old and its owner lives `another flush is
  running: sender stuck since <t> (pid <n>) - its lock is <m> min old and its owner lives: it is
  never taken over; stop pid <n> if it hangs (the lock goes with its process)` - writing `sender
  stuck since <t> (pid <n>)` into the record's notes ONCE (the existing `add_last_note` dedup); the
  next sender that holds the lock drops that note (already C3's rule). `c3 telemetry --status`
  prints `sender     : busy since ...` / `sender stuck since ...` / a record whose owner is gone.
- **Decided differently:** the owner record adds `host` (the machine name) to the plugin's four
  fields - local only, never sent; the plugin's lock-file record is a separate file in C3 (above).

Tests: `telemetry::spool::tests` (the answer classes, `events[i]` parsing, `Retry-After` as
seconds and as a date, the result line, the seconds text); `crates/c3/tests/telemetry.rs` against
a scripted intake with headers (`intake`) or an injected one: `sender_429_with_a_short_retry_after_
resends_the_same_batch_once` (two requests, the same body, >= 1 s apart, the record),
`sender_429_with_a_long_or_no_retry_after_keeps_the_batch` (one request, the spool byte-identical,
the record's result and `http` 429), `sender_429_again_or_not_fitting_the_deadline_stops`,
`sender_400_drops_the_named_event_and_resends_the_rest` (the second body without event 1,
`rejected[]`, `dropped` 1), `sender_400_a_fourth_refusal_in_one_flush_stops_it`,
`sender_413_halves_the_batch_and_drops_an_event_too_large_alone` (batch sizes 4, 2, 1, 1, 1, 1),
`sender_sends_every_event_in_batches_of_at_most_100` (100 + 50, oldest first),
`sender_stops_at_the_flush_deadline_and_keeps_the_rest`,
`sender_403_non_json_and_other_refusals_stop_and_keep_the_spool`,
`sender_owner_record_lives_with_the_lock` (the record's six keys while a flush holds the lock, the
second sender's and `--status`'s "busy since", gone afterwards),
`sender_stuck_for_30_minutes_is_noted_once_and_dropped_when_free`.

## 4 - the version

The workspace version is `0.2.0` (`Cargo.toml` `[workspace.package]`, inherited by `c3-core`, `c3`
and `c3-cli`; `c3 --version` prints `c3 0.2.0`; the telemetry `app_version`, the MCP server info and
the router's user agent follow `CARGO_PKG_VERSION`); the plugin manifest
(`plugin/.claude-plugin/plugin.json`), its marketplace entry (`.claude-plugin/marketplace.json`) and
`plugin/README.md` carry 0.2.0; `CHANGELOG.md` (new, Keep a Changelog) lists the parity track's
waves with the finding ids they closed; the README's status paragraph names the parity with
claude-codex-consult 0.6.1. Not tagged.

## Harnesses through the shim (pinned v0.6.1)

Staging as in `harness-shim.md` section 4 (the v0.6.1 plugin tree with the six shims over it; the
consult shim now forwards `-BriefPrefix`), host Windows PowerShell 5.1 with `PSModulePath` reset,
one harness at a time under `%TEMP%\codex-consult-tests\HARNESS.lock`. `C3_EXE` was a copy of this
branch's build: harness-host and harness-fixes27c on the final build (`c3 0.2.0`, commit 2's code),
harness-telemetry, harness-0.3 and harness-panel on the build of commit 1 (`c3 0.2.0`; the only
change after it is the coordinator's dry-run and D11 console text, which neither of those three
reads). The "before" numbers are main's (state log 06:20 / 08:00 and the latest runs of the day).

| harness | before | after | what changed |
|---|---|---|---|
| host | 55 / 10 | **59 / 6** | PREFIX D6 x2, REFUSE D3 and WARN D3 (the real run in a claude-code host) green |
| telemetry | 47 / 96 | 47 / 96 | the same failure set (diffed): every sender row stays by design, below |
| 0.3 | 229 / 0 | 229 / 0 | no regression |
| panel | 62 / 0 | 62 / 0 | no regression |
| fixes27c | 34 / 2 | 34 / 2 | no regression (run for the coordinator-text change: COORD D10-D12 green) |

**harness-host - the six that remain are not applicable to C3** (`harness-shim.md`, "Not
applicable to C3"; the single-host decision of 2026-09-29): WARN D3 dry run and WARN D3/F04-10
expect `host codex` inferred from `CODEX_SESSION_ID` (every other condition holds - the warning
once, the preview's `warnings[]`, the coordinator `{openai, gpt-5.1, codex, explicit}`, the key
order, `child_env_scrubbed [CODEX_SESSION_ID]`), WARN wave 27b expects `host zcode`, ENV D4/27b,
ENV D4 detached and ENV D3/D4 panel expect `coordinator.host codex`.

**harness-telemetry - the sender rows (SEND x2, R429 x2, BATCH, DROP, LOCK x2, NONJSON x3, D8 x5,
D2 x4, D4 x3, D5, URL) remain by design (P7):** they seed lines into the PLUGIN's
`<codex home>/telemetry-spool/*.ndjson` (events of `app_id` `codex-consult`, which C3's closed
classes would discard even if it read them) and count the requests to the harness's intake; C3
reads only its own outbox `<codex home>/c3/telemetry/spool.ndjson`, so its flush answers "nothing
to send" (now in the plugin's own words) and the intake sees no request. SEND x2 also expects the
event of a run delivered by a detached sender right after the run - C3 sends the outbox from a
thread at the NEXT consultation's start (or `c3 telemetry --flush`). The fixes28d LOCK D3 rows run
the plugin's own `Enter-TelemetryFlushLock` in-process (the plugin's `.flush.lock` record); C3's
owner record is its own file. The behaviour those rows describe - 429 Retry-After, 400 `events[i]`,
413 halving, 403, the deadlines, batches of 100, the owner record and "sender stuck" - is ported
and covered by the Rust tests of section 3 against a fake intake.

## Tests and checks

`cargo test --workspace` (run target by target, each target's PDB deleted after it - disk C had
5 GB free and a test PDB is 300-800 MB): 737 tests, all green; two known flakes seen once and green
on the rerun, both in code this wave does not touch - `index_embeddings`
`rebuild_then_embed_reproduces_the_vector_count` (a SurrealKV "transaction write conflict") and
the lib test `index::embed::tests::embed_reaches_a_loopback_fake_via_localhost` (while harnesses
ran their loopback intakes). `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --check` clean. A shared-target hazard met on the way: `target/debug/c3.exe` is uplifted
by whichever worktree linked last, and the c3-cli tests run THAT file (`CARGO_BIN_EXE_c3`) - the
c3-cli suite was re-run after touching `crates/c3-cli/src/main.rs`, with `c3 --version` = 0.2.0
checked before and after.
