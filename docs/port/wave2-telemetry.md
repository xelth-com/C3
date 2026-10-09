# Wave 2 of the 0.6.1 parity: telemetry (2026-10-08/09)

Branch `wave2-telemetry`, three commits: **2a** the 0.6.1 rating semantics and the ledger shape,
**2b** a durable outbox and the forget-me retry identity (Astra's F02-1, F02-3), **2c** closed
telemetry classes and the telemetry shim (F02-2, F02-4); then branch `wave2d-fixes` (**2d**, Astra's
F09-1..F09-6 - section "2d" below). The specification is the plugin at
`v0.6.1` (`codex-consult-common.ps1` telemetry section, `codex-findings.ps1 -Rate`,
`codex-telemetry.ps1`, `codex-consult.ps1`'s ledger entry, CHANGELOG `[0.6.1]`/`[0.6.0]`, README
"Telemetry (on by default)").

## 2a - what was ported

**The ledger entry** is the plugin's 0.6.1 literal key for key (`crates/c3-core/src/ledger.rs`;
harness-0.3 LEDGER `$order`): `consult_ref` right after `consult_id` (a fresh random lower-case
guid minted by the process that commits the entry - a panel member mints its own; derived from
nothing), `context_window` after `extra_config_source`, `compactions` after `usage`. All three are
tri-state (absent in an older entry and kept absent on rewrite; a fresh entry writes them in
position), so `tests/formats.rs` still round-trips the design evidence byte for byte.

- `context_window` (the coordinator's addendum; plugin wave 28b D15): a codex reviewer whose roster
  entry names `context_tokens` n gets `-c model_context_window=n` and
  `-c model_auto_compact_token_limit=floor(0.8 n)` on EVERY codex turn (main, continuation,
  repair) - a key the operator's `-CodexConfig` / roster `codex_config` already sets (`^key\s*=`,
  any case) is not repeated; the ledger records `{tokens, auto_compact_limit, items}`, else null.
- `compactions` (plugin wave 28c D11): the compaction events every turn's stream REPORTED
  (`context_compacted` / `compacted` / `thread.compacted`, a `msg` of those, `system`
  `compact_boundary`, an `item.completed` of `context_compaction` / `contextCompaction` /
  `compaction`; a codex repair turn's temp stream is counted before it is removed): n > 0 is
  recorded and warned about ("the reviewer compacted its context n time(s) - the reply may rest on a
  summary of the brief", in `warnings[]` and as a `warning    :` line), none with a context window
  is `"unknown"`, else null.
- The dry-run preview follows the plugin's literal order and gained `topics`, `role`,
  `consult_ref`, `context_window`, `format_retry` (the plugin's placeholder; null with the repair
  off) and `compactions`.

**`c3 findings --rate`** (`findings_tool::mode_rate`): the judge is resolved AT RATING TIME -
`CODEX_CONSULT_COORDINATOR` of the rating process parsed as the bridge parses it
(`host::parse_coordinator_matcher` + `build_coordinator`, the roster for labels and `#n`) ->
source `rating_actor` (a value that does not parse rates as `other/other`); else the entry's own
`coordinator` when it names anyone -> `consult_coordinator`; else `{other, other, unknown}`.
`rating_rev` = 1 + the consultation's highest (`c3_core::findings::next_rating_rev`, a mark
without one counts as 0), allocated under the task lock in the mark's commit. The mark
`{n, consult_id, lineage, provider, model, engine, purpose, topics, consult_when, useful, note,
when, rating_rev, judge}` is written with telemetry on OR off; it is committed BEFORE its event is
spooled (1 s wait, still inside the write lock), and `telemetry_sent` (unix seconds) is written
under the same lock when the spool succeeded. A failed spool is retried for 5 s after both locks
are released (the SAME event; `telemetry_sent` then through a new store commit, or "rated again
meanwhile" when the mark was replaced), then warned about, counted (`not-spooled.ndjson`) and
left for the backfill. `--telemetry on|off` per rating (the plugin's refusals). Test hooks (test
mode only): `CODEX_CONSULT_TEST_RATE_ABORT_AFTER_COMMIT=1` (exit 87 between the commit and the
spool), `CODEX_CONSULT_TEST_RATE_RETRY_GATE=<file>`.

**`c3 telemetry --backfill-ratings [--dry-run] [--collab-dir] [--telemetry]`**
(`telemetry/backfill.rs`): every mark of every task without `telemetry_sent`, looked up by
`consult_id` (else `n`; none: skipped and counted), sent with the MARK's judge (saved; a mark
without one: the consultation's coordinator or unknown - never the backfill process's
`CODEX_CONSULT_COORDINATOR`), `rating_rev` and `when`, `telemetry_sent` written under the task's
write lock, once. Output lines as the plugin's (`codex-telemetry: <task>: sent N, already M,
skipped K`, the total, the dry run's `would send: <class> / <model> (<engine>), purpose ..,
mark .., age_days .., client_time .., judge <class> / <model> (<source>)`); telemetry off: refused
(exit 1).

**The switch** is the plugin's `Get-TelemetrySwitch`: a run's `--telemetry on|off` wins (C3 used to
let the variable win); unset/empty on; `on 1 true yes` on; `off 0 false no none` off; any other
value off. `c3 consult --telemetry` defaults to "the environment decides" (it defaulted to `on`),
and a panel passes its switch to its members (it did not). **The intake** honours
`CODEX_CONSULT_TELEMETRY_URL` with the plugin's rule (https; plain http only to a loopback host in
test mode; anything else refused - nothing sent); `C3_TELEMETRY_HUB` stays C3's own test override.

## The events as sent

Consultation event (C3's own `app_id` `c3`; the top level now in the plugin's order):

```
{app_id:"c3", app_version, instance_id, event_type:"consultation", severity:"info|warning|error",
 title:<purpose class>, details:{engine, provider, model, purpose, outcome, wall_seconds, tokens_in,
 tokens_out, findings, structured, format_retry, panel_size, peers_used, os, runtime,
 consult_ref?}, tags:[provider, model], client_time, os, runtime}
```

Rating event (the plugin's 0.6.1 details exactly):

```
{app_id:"c3", app_version, instance_id, event_type:"rating", severity:"info", title:<mark>,
 details:{engine, provider, model, purpose, mark, age_days, bridge_version, os, ps_version:"unknown",
 judge:{provider, model, source}, rating_rev?, consult_ref?}, tags:[provider, model],
 client_time:<the mark's when, UTC>, os, runtime}
```

C3's M9 `topic_tags` are no longer in the rating event (the plugin's details never carry the
topics; the brief asked for the plugin's exact keys). If the intake's priors need topic cells from
C3 ratings, they come back as an additive key after `consult_ref`.

## 2b - the outbox (F02-1)

Files under `<codex home>/c3/telemetry/` (`telemetry/spool.rs`):

- `spool.ndjson`: one line per item, the plugin's spool line `{"v":1,"kind":"event",
  "queued_unix":<s>,"body":"<event JSON as a string>"}`; the 7-day drop runs from `queued_unix`
  (a backfilled mark's old `client_time` is not dropped); a raw event line C3 wrote before wave 2
  is still read and sent - through the closed classes since 2d (F09-5); another kind is kept
  untouched.
- `spool.lock`: THE spool lock (`File::try_lock` - `LockFileEx` / `flock`, released by the OS
  when the holder dies: never stale). Held by every append (a producer waits at most its budget:
  1 s at a rating's commit, 5 s otherwise; it builds its event and the instance id UNDER the lock),
  by the sender's snapshot read and its rewrite, and by the local deletion of a forget.
- `flush.lock`: the sender lock, held for a whole flush; a second sender that finds it busy skips.
- `last-flush.json`, `not-spooled.ndjson` (one `{time, why}` line per event not spooled, reset by a
  flush that ran), `forget-pending.json` (below).

The rule: (1) under the spool lock, read a snapshot; (2) drop unreadable and stale lines, batch up
to 100 events, POST; (3) under the spool lock again, re-read the CURRENT file and remove exactly
the delivered and dropped lines (a multiset of exact lines, one occurrence each), writing the
remainder to a temp file renamed over the spool. An event appended between (1) and (3) is not in
the multiset and stays - the lost-update race of F02-1 is gone (test
`outbox_keeps_an_event_appended_while_the_sender_posts`: the intake holds its answer to A while B
is appended; B is still queued afterwards). Crash safety: before the rename the old file is intact
(a crash after a successful POST re-sends: at worst a duplicate, never a loss); a torn append
leaves a line without its newline, which the next append ends first and the sender drops.

**Why C3 does not write the plugin's `telemetry-spool/`:** C3 is its own app at the intake
(`app_id` `c3`, counted at xelth.com/C3/) with its own salt and instance id. Sharing the plugin's
directory would mix two apps' identities in one outbox (a plugin `-Forget -Local` would delete
C3's queued events and vice versa) and would require the plugin's whole interlock - the
`FileShare.None` producer opens, the `.flush.lock` owner/token record, the per-producer
not-spooled files and the `.last` fold, the forgetting marker - bit for bit (wave 3's E2/E3/E20/
E24/E26). Whether the two bridges should share one salt/instance identity is the open question of
Astra's Q3 (handoff 02); until it is decided, C3 keeps its own root and borrows the plugin's line
format and removal rule so a later move is a directory change plus that interlock.

## 2b - forgetting (F02-3)

`telemetry::forget_at` (`c3 telemetry --forget [--public-ref] [--local] [--yes]`, the plugin's
`-Forget`; `c3 forget-me` = the same with `--local`, the reference defaulting to a pending
deletion's or the newest stored one):

- with a reference: the intake FIRST (`DELETE <intake>/v2/instances/<id>?public_ref=<ref>`, the
  instance id a pending deletion's when one is recorded, else this salt's); confirmed = a 2xx that
  is not `{"ok": false}`;
- NOT confirmed (`HTTP 404: unknown public_ref`, a 5xx, unreachable): NOTHING is deleted - the salt,
  the outbox and the references stay; `forget-pending.json` records `{instance_id, public_ref,
  since, attempts, last_error}`; exit 3 and a message that says exactly that. While the record
  exists nothing is spooled (a producer's event is refused under the spool lock and counted) and
  nothing is sent (the sender skips). The retry uses the record's identity even when the salt is
  gone (test `forget_failed_delete_keeps_the_identity_and_the_retry_uses_it`);
- confirmed: the record goes; with `--local` the local files go (salt, outbox, references,
  counters, last flush) - under the sender lock and the spool lock, so no sender posts and no
  producer appends meanwhile; (2d, F09-2/F09-3: the record is a deletion TRANSACTION written
  before the DELETE, every forget holds both locks, the cleanup is resumable - see "2d");
- `--local` without a reference: says that the intake keeps what was sent (instance id) and asks
  (`--yes` skips); a pending deletion record survives it (it carries the identity for the retry).

The old message "the server drops it on the next successful DELETE" is gone.

## 2c - the classes (F02-2)

`telemetry/classes.rs` ports the plugin's closed vendor table (`openai` incl. Codex's built-in
provider, `zai`, `xiaomi`, `byteplus`, `moonshot`, `alibaba`, `minimax`, `google` = agy, `meta` =
muse, `anthropic` = claude; hosts by equality or `.`-suffix; the exact closed model lists, `[1m]`
stripped, lower-cased both sides), `Get-TelemetryReviewerClass` (one code path for the
consultation and the rating event), `Get-TelemetryPurpose` (eight purposes, `none`, `other`),
`Get-TelemetryOutcome` (`usable`, `usable-after-continuation`, `failed:<class>` with the closed
class list, the severity), the judge classifier and its allowlist. C3 extensions: the engine
`http`; the class `openrouter` for `openrouter.ai` with the model in OpenRouter's `<slug>/<model>`
form only when the slug is one of `openai anthropic google z-ai moonshotai minimax xiaomi qwen
meta-llama` AND the model part is in that class's list, else `other`.

`safe_label` no longer produces any event field (it was the F02-2 leak: `customer-acme` passed
it); it survives only as a pre-filter before the closed topic vocabulary. A complaint's last-run
summary uses the classes too. Found on the way: C3 sent every usable consultation as
`failed:unknown` (its outcome check did not know the bridge's `usable reply`); the plugin's rule
fixes it, and the severity now follows the outcome.

## 2c - the telemetry shim (F02-4)

`tests/shim/codex-telemetry.ps1` (see `harness-shim.md` section 4): the plugin's parameter names
onto `c3 telemetry` / `c3 complain`; the consult and findings shims forward `-Telemetry`.

## 2d - Astra's wave 2 diff-review (F09-1..F09-6, handoff 09)

Branch `wave2d-fixes`. Each finding has the fixture Astra asked for (RC1-RC5); the RC2-RC4 tests
were also run against the old behaviour re-introduced by hand (the unlocked-only deletion check,
the reread error read as an empty file, the unclosed body, the cleanup branch skipped) and fail
there.

**F09-1 - the context-window keys (RC1).** `context_window_config` sliced each operator item at the
byte length of `model_context_window` - a char-boundary panic on `notify=["日本日本日本日本"]`. It now
splits the trimmed item at its first `=` and compares the trimmed key (ASCII case-insensitive) -
the plugin's `^key\s*=`. Test `context_window_config_survives_a_unicode_operator_item` (both
defaults added; a Unicode key of the same byte length matches nothing).

**F09-2, F09-3 - the deletion transaction (RC2).** `forget-pending.json` is a transaction now:
`{instance_id, public_ref, since, attempts, last_error, phase}` (a record without `phase` - written
by wave 2 - is `pending`).

- `pending` is written BEFORE the DELETE (attempts counted then); `confirmed` when the intake
  confirmed a `--local` forget; `cleaning` when the local cleanup starts (a local-only forget starts
  here, `public_ref` empty); the record is removed only AFTER the cleanup finished. A forget without
  `--local` removes it right after the confirmation (the plugin keeps the salt then as well).
- The cleanup removes the QUEUED data first - `spool.ndjson`, a rewrite's leftover
  `.spool.ndjson.*.tmp`, `not-spooled.ndjson`, `last-flush.json` -, then the proof
  (`refs.ndjson`), the identity (`salt`) LAST, and the salt only when it still is the
  transaction's instance (a salt made meanwhile by a complaint is another instance: kept, said).
- EVERY forget (with or without `--local`) holds the sender lock and the spool lock from its first
  write of the record to its last, across the DELETE (producers wait their budget and are refused,
  as a `--local` forget did before). The sender decides on the record UNDER both locks (its early
  unlocked check is only a shortcut): `pending` or unreadable - skip; `confirmed`/`cleaning` - it
  finishes the cleanup with the record's identity instead of sending ("finished the local deletion
  of instance ..."). A producer is refused under the spool lock in any phase.
- `c3 forget-me` / `--forget` with a `confirmed`/`cleaning` record only finish the cleanup (no
  DELETE, no question); `--status` names the phase; a record that cannot be read blocks (fail
  closed) until a forget rewrites it.
- Known limit (unchanged): a DELETE the intake executed but whose answer was lost stays `pending`;
  the retry may then get `404 unknown public_ref` forever. Abandoning a pending deletion is not
  offered - an operator decision for later.

Tests (`crates/c3/tests/telemetry.rs`): `a_forget_failing_while_a_flush_waits_never_lets_the_flush_post_rc2`
(a barrier after the sender's early check, a failed forget with and without `--local`, the injected
sender never called), `a_confirmed_delete_with_an_interrupted_cleanup_resumes_and_never_posts_the_old_instance_rc2`
(confirmed DELETE, cleanup interrupted where the spool goes - the record `cleaning` with the old id,
the salt still there -, then the review's state: salt removed, spool preserved; restart: a producer
refused, the flush posts nothing and finishes the cleanup, the intake saw only the DELETE, the next
event is a new instance), `a_confirmed_transaction_is_finished_by_the_next_forget` (a `confirmed`
record finished by a forget without a reference; a salt of another instance kept). Test seams:
`Spool::flush_hooked` + `FlushHooks {after_check, reread}` and `telemetry::forget_with` (an
injected file removal), `#[doc(hidden)]`, the pattern of `flush_with`.

**F09-4 - the reread rule (RC3).** After a POST the sender re-reads the CURRENT spool for its
rewrite; a reread error is returned (the flush fails, `last-flush.json` says so) and NOTHING is
replaced - the delivered lines go again next time (a duplicate at worst, never a loss). NotFound is
the one exception, by an invariant: only the local deletion removes the spool and it holds the
sender lock the flush holds, so a missing spool was removed from outside - nothing to keep, nothing
rewritten, no file made. Test `outbox_reread_failure_after_a_send_keeps_the_spool_bytes_rc3`.

**F09-5 - the upgrade backlog (RC4).** Every queued event is closed before it leaves
(`classes::close_event_body`): re-built field by field in the constructors' key order - engine of
`ENGINES`, provider a vendor CLASS as it stands (a label such as `customer-acme` or `ZAI` is `other`
- labels are not resolved through today's config), model through that class's list (`unknown`
kept), purpose/outcome/mark/judge/os/versions through their closed sets (`failed:<not a class>` is
`failed:unknown`, severity from the outcome), `consult_ref` only as a guid; a rating gets the 0.6.1
detail keys (`bridge_version` the event's version, `ps_version` unknown, a missing judge
`other/other/unknown`; the old `topic_tags` dropped). An event of the current constructors closes
to itself and is sent with its exact bytes (unit test `a_current_event_closes_to_its_exact_bytes`).
An event that cannot be attributed - another `app_id`, no 64-hex instance id, no RFC 3339 time, no
consultation/rating - is discarded unsent with a local diagnostic (`FlushReport.discarded`, the
debug log, `last-flush.json`, `--flush`). This covers raw pre-wave-2 lines AND envelopes a 2b-only
build may have queued. Test `legacy_queued_events_never_send_a_private_label_rc4`.

**F09-6 - one coordinator resolver (RC5).** `c3_core::host::resolve_coordinator_identity` (with
`resolve_coordinator_match` and `codex_config_defaults` - `Get-CodexConfigDefaults`) is the plugin's
`Resolve-CoordinatorIdentity` and the ONE resolver of the consultation's ledger `coordinator` and of
a rating's actor: a bare label takes its roster entries' one model and engine (a model-less codex
entry counting as the config's model), several models leave it unnamed, a label outside the roster
takes the config's model only for the config's provider (ordinal, codex engine), `#n` of a
model-less codex entry the config's model, a lineage without an engine its first entry's engine
(else codex), a Claude id's `[1m]` stripped. The old `parse_coordinator_matcher` is gone. Visible
side effect (plugin parity): the ledger record of `openai :: gpt-6-astra` now carries `engine:
"codex"` (was null) and a bare label in the roster its model, so the coordinator warning says "own
model" where it said "own provider (model not named)". Tests `rating_actor_of_a_bare_label_infers_its_sole_roster_model_rc5`
(parity: `JudgeLabel-Kimi` -> mark and event `moonshot/k3/rating_actor`, ledger `k3`/`codex`; the
two-model label `ZAI` -> `zai/other`; bare `openai` -> the config's `gpt-5.1`),
`coordinator_resolution_infers_models_like_the_plugin`, `codex_defaults_from_the_config`.

Workspace: 576 -> 586 tests (`cargo test --workspace`), clippy `-D warnings` and `cargo fmt --check`
clean. `index::embed::tests::embed_reaches_a_loopback_fake_via_localhost` failed once under the
parallel suite and passed alone (untouched code, a loopback timing flake).

- **2f, F14-1 (handoff 14, Kimi's RC1).** A local-only forget (no reference, no record) writes its
  `cleaning` transaction BEFORE the first removal (a record that cannot be written: nothing removed),
  so an interrupted cleanup blocks spooling and sending and the next flush or forget finishes it; the
  failure message names the record only when it exists. Tests `a_local_only_forget_interrupted_keeps_its_record_and_resumes_rc1`,
  `a_local_only_forget_leaves_no_record_behind`, `a_local_only_forget_that_cannot_record_removes_nothing`.
- **2f, F14-2.** The resolver's "ordinal" is the plugin's `-ceq` (`Test-ReviewerMatch`,
  `Resolve-CoordinatorIdentity`): the provider stays CASE-SENSITIVE (`OpenAI` is not `openai`, as in
  C3's `-Require` matcher) - the doc now says so; the engine compares case-insensitively (`-eq`).
  Test `coordinator_provider_is_case_sensitive_the_engine_is_not`. Workspace 610 -> 614 tests.

## Tests

Rust: `crates/c3-cli/tests/telemetry_parity.rs` (the real binary against a fake codex: ledger
order and `consult_ref`; the switch; CONTEXT/COMPACT; RATE with three judge sources, rating_rev
1..5, telemetry_sent, telemetry off, the refusals, RC1 abort-then-backfill under another
coordinator, backfill once, refused off; a legacy mark without a saved judge; `--status` and the
form refusals), `crates/c3/tests/telemetry.rs` (allowlists incl. `customer-acme` everywhere, the
outbox race, one sender, the line shape / legacy / torn line / queue-time drop, the failed DELETE
and its retry, the -Forget refusals and -Local alone), `telemetry::classes` unit tests (host
table, closed lists, judge classifier, OpenRouter, reviewer/purpose/outcome classes,
`consult_ref` shape). Workspace: 558 -> 576 tests, clippy clean.

## Harnesses through the shim (pinned v0.6.1, 2026-10-09)

Staging as in `harness-shim.md` section 4 (the six shims incl. the new `codex-telemetry.ps1`, the
v0.6.1 `codex-consult-common.ps1`/`-detached.ps1`, `../schemas`), `C3_EXE` a copy of the wave-2c
build, one harness at a time under `HARNESS.lock`, host **Windows PowerShell 5.1** (RC2 ran pwsh
7.6.6). The first run (all ten) used the 2c build before its last three small fixes (`--telemetry`
validation, the dry run's telemetry line, `--flush` refusing a refused intake URL); telemetry and
muse were run again on the final build.

| harness | before (RC2 / brief) | after | what changed |
|---|---|---|---|
| telemetry | 23 / 20, crash at line 315 | 45 / 98 (first run 43 / 100) | finishes now (no crash); SPOOL/RATE/BACKFILL/STATUS/FORGET reach C3 through the shims |
| roster | 121 / 4 | 124 / 1 | WALK, RATE x2 green |
| 0.3 | 226 / 3 | 227 / 2 | LEDGER order green |
| companions | 32 / 10 | 35 / 7 | ROLE ledger order, RATE record, ROUTED D6/D9 green |
| engines | 86 / 11 | 87 / 10 | RUN ledger order, SCOREBOARD green; RUN A8 new (5.1 host, below) |
| muse | 60 / 14 | 62 / 12 (first run 60 / 14) | RUN ledger order green; TREE x2 green on the rerun (flaky: the first run's agy member failed with `could not register the agy process (os error 5)`, a rename racing a reader) |
| panel | 60 / 2 | 60 / 2 | no regression |
| format | 36 / 1 | 37 / 0 | CONTR green |
| fixes28b | 12 / 8 | 13 / 7 | CONTEXT "no -c option, context_window null" green |
| fixes28c | 10 / 5 | 13 / 2 | COMPACT x3 green |

### Every remaining failure, classified

**harness-telemetry** (98 failures):

- **C3 by design - its own outbox, app and identity** (`<codex home>/c3/telemetry/`, `app_id` `c3`,
  its own salt): every check that reads the plugin's `<codex home>/telemetry-spool/*.ndjson`,
  `.last`, `telemetry-salt` or the plugin's instance id, or walks an event against the plugin's
  `codex-consult` allowlist - SPOOL x14, RATE x14, BACKFILL x17 (C3's printed lines match the
  plugin's - `bf-task-one: sent 1, already 0, skipped 1` - the spool count does not), STATUS x2
  (the seeded plugin spool; the fresh-home check fails only on "the notice not shown": C3's notice
  marker lives in the real home, so scratch homes never show it), FORGET x7 (the harness creates the
  PLUGIN's salt: C3 sees no instance id), NOTICE x1 (the plugin's per-version notice), SEND x2 / ENV
  x1 (C3 sends from a thread at the next consultation's start, not a detached sender process with
  an allow-listed environment), and the sender checks R429 x2, NONJSON x3, DROP x1, LOCK x2, BATCH
  x1, D8 x5, D2 x4, D4 x3, D5 x1, URL x1 (C3 now refuses the flush before any connection, exit 1;
  the check also wants the plugin's `.last`) - they seed the plugin's spool, which C3 never reads. Behind
  those sender checks is a real gap that is NOT wave 2's: C3's sender does not implement the plugin's
  intake-answer handling (429 Retry-After, 400 `events[i]` drop, 413 halving, the 60 s flush deadline,
  the owner/token lock record) - a later sender-parity item.
- **Wave 3 (not-spooled count / fold / forgetting marker):** SPOOL D6/D7 x2 (the busy-spool warning
  text, the 1 s + 5 s timing on the plugin's spool file, `-Status` "not spooled: 1"), FORGET D3 x1
  (the forgetting marker of a living owner).
- **Shim artifact:** DRYRUN "on (the default)" - the consult shim sets `CODEX_CONSULT_TELEMETRY=off`
  when the caller left it unset (its safety net), so the dry run can only say off.
- **Older gap, not wave 2:** COMPLAIN x10 (the consult shim has no `-Complain`; C3's complaint is
  `c3 complain` with its own payload, no `-Contact`/`-Task`, no spooled retry of an undelivered
  complaint), HOOK x1 (C3's SessionStart hook prints no coordinator/telemetry pointer line).
- **Documentation:** DOCS x2 (the plugin's README/skills/manifest are not in the staging).
- **Still wave 2:** none (DRYRUN "off (-Telemetry)" / "off (CODEX_CONSULT_TELEMETRY)" and "-Telemetry maybe refused" went green on the rerun).

Totals: by design 81, wave 3 3, shim artifact 1, older gaps 11, documentation 2.

**Other harnesses:**

- roster FILE - wave 4 (the claude engine's roster keys `claudeauth`/`claudemodel`).
- 0.3 LEDGER reviewer fields (the names match; the check also wants the test-mode warning in
  `warnings[]` - fixes28b TESTMODE D10) and CFG (the `-CodexConfig` comma split) - older gaps (RC2
  triage).
- companions ROUTE D8 (the plugin's `templates/` not staged - shim artifact), SIZE D6, ROUTED D8,
  REQUIRE x2, ROLE D8, RATE `-By topic` - older gaps (RC2 triage item 5, the scoreboard).
- engines ROSTER x2, DRYRUN x5, RUN thread/usage (test-mode warning) - older gaps; RUN argv - the
  launcher quoting below; RUN A8 - **host artifact**: under Windows PowerShell 5.1 the shim's
  `& $c3 @c3Args` passes an argument with embedded `"` and a newline unescaped (pwsh 7 passed it in
  RC2).
- muse UNIT D2, ROSTER (wave 4: the claude engine name), DRYRUN x2, ENGINEEXE, RUN x3, REPAIR, PANEL x3 -
  RC2's failures without the ledger order (and TREE, flaky) - older gaps.
- panel SPEC x2 - older (recovery, wave 3).
- fixes28b TESTMODE x2, HEALTH x3 - older gaps (D10, D13); **CONTEXT x2 - launcher quoting, not
  semantics:** C3 passes `-c model_context_window=256000` (the ledger records it - Rust test
  `context_window_reaches_codex_and_compactions_are_recorded`), but Rust std's batch-file argument
  escaping quotes every argument that holds `=` when the launcher is a `.cmd` (`-c
  "model_context_window=256000"`, also `"-p="` for agy), and the harness matches the fake's raw `%*`
  text unquoted (`(^| )-c model_context_window=256000( |$)`). Remedy (a spawn-path change outside
  this wave): pass plain arguments (letters, digits, `._-:/\=+@`) to a batch launcher with
  `CommandExt::raw_arg`, keep std's escaping for everything else; it would also clear engines RUN
  argv. Left for wave 2b/the triage list.
- fixes28c IDENTITY D8 (wave 3), JOURNAL D10 (older: the health journal's `.bad` line).
