# Wave 9 - the four defects of the 2026-10-14 consilium panels

Found in the consilium panels of task `consilium-2026-10-14`; fixed on main for the 0.2.2
candidate. The plugin is the oracle only where it does not contradict a fix: the plugin harness is
run through the shims, and the two places where it pins the old behaviour are kept behind the
shims' compat switch `C3_PLUGIN_COMPAT=1` (D1, D2) - a normal `c3` run never sets it.

## D1 - a panel with a new brief forked the previous threads

Trigger: `c3 consult --panel --panel-all` on a task that already had threads (second brief) forked
every codex member from the first panel's threads (0.8-1.2 MB each); the fork guard
(`CODEX_CONSULT_FORK_MAX_KB`) then refused four members. Rule: a panel defaults to `--mode new` for
every member unless `--mode fork` is given (then the guard applies as before; `--mode resume` stays
refused for a panel); the dry run and the panel header print `mode: new - every member starts a new
thread ...` / `mode: fork - ...`. The plugin forks the newest thread of each lineage by default and
its `harness-roster` (PANEL: "second panel: openai forks the openai thread, mimo the mimo thread")
pins that, so under `C3_PLUGIN_COMPAT=1` (set by `tests/shim/*.ps1`) the mode stays automatic and
the header is the plugin's. Test: `wave9_consilium::a_panel_starts_new_threads_by_default_and_forks_only_when_asked`
(panel B starts a new thread, `--mode fork` forks it, a 1 KB cap refuses the heavy 4 KB parent).

## D2 - the detach "come back" line printed PowerShell commands

Trigger: `codex-consult.ps1 -Task <t> -Status -Id <id>` / `-Wait` in the detach output. Rule: C3's
own lines name `c3`: `c3 consult --task <t> --status --id <id>` and `--wait --id <id>` (the `-List`
line of the SessionStart hook and `c3 findings --list`, the unknown-finding hint
`c3 findings --task <t> --list --all`, every "run codex-providers.ps1 for the full picture" as
`run c3 providers`). New module `c3_core::spelling`. The ledger/handoff FILE formats are untouched
(the handoff's `Invocation: codex-consult.ps1` line stays; the hook's `-Explain` pointer is the
plugin's). Pinned by the plugin harness (`harness-detach` SINGLE "come back  : codex-consult.ps1
-Task t -Status -Id ...", `-List` lines, `harness-roster` refusals), so under
`C3_PLUGIN_COMPAT=1` the old spelling is printed. Test:
`wave9_consilium::detached_single_runs_of_one_task_run_side_by_side_and_speak_c3` (the output has
`c3 consult --task t --status --id` and no `.ps1`).

## D3 - detached single runs of one task serialised on `.consult.lock`

Trigger: four `--detach` single runs of one task: two ended "skipped: not started: another
consultation is running". Rule: a DETACHED single run holds the task lock only for its bookkeeping
(it retries the lock for up to a minute, takes its numbers `n`/`nn` under it, writes a member-style
recovery record `.consult.pending-<NN>.json` and releases it), then runs the reviewer turn without
the lock, as a panel member does under its panel; the commit goes through the write lock
(`commit_wait_ms`) exactly as before, so the ledger numbering stays strictly increasing and
`sessions.json` / `findings.json` are never written concurrently. A live record of another
detached single run (a member-style name without a `parent_pid`) is no obstacle to a detached run
(`Recovery.concurrent`); a foreground run still refuses on it. Only `--detach` changes: a
foreground single run still holds the lock for the whole run (the plugin's behaviour; no harness
test runs two detached singles of one task). Test: `wave9_consilium::detached_single_runs_of_one_task_run_side_by_side_and_speak_c3`
(two detached runs with a 6 s fake turn: their `[when, finished_at]` intervals overlap, distinct `n`).

## D4 - an instant 429 on a plan with a monthly cap was a 10-minute burst

Trigger: alibaba (Qwen) answered `429 Too Many Requests` in 0 s twice while the plan's monthly quota
was at 100 % (reset 2026-10-27). Rule: a roster entry may declare its plan's cap under its
`ext.c3` (the plugin validates `ext` as an object and ignores it): `"ext": {"c3": {"plan_caps":
["monthly"], "reset_hint": "monthly:27T00:00+08:00"}}` (`plan_caps` of `monthly` | `weekly` |
`daily`; `reset_hint` `monthly:<DD>T<HH:MM><offset>`, `weekly:<Mon..Sun>T<HH:MM><offset>` or
`daily:<HH:MM><offset>`). Two instant 429s (a burst-class failure, wall under 2 s, no window text)
within 60 minutes of each other and newer than the endpoint's newest usable reply mark the entry
`unavailable (plan quota: 429 twice in <n> min, no window text; retry after <next reset>)` until the
hint's next reset, else 24 h after the second hit - instead of the 10-minute burst
(`c3_core::plan::plan_cap_verdict`, applied by `Ctx::plan_verdict`, so `c3 providers`, the walk, the
panel and a direct run all see it). An entry without the declaration keeps the 10-minute burst.
Tests: `plan::two_instant_429s_inside_an_hour_mark_a_capped_entry_out_until_the_reset`,
`plan::the_cap_reset_hint_names_the_next_reset`,
`roster::an_entry_declares_its_plan_cap_under_ext_c3`,
`providers::two_instant_429s_on_a_capped_plan_mark_the_entry_out_until_the_reset`.

## 9b - the review of wave 9 (handoff 66: F66-1 .. F66-6)

**F66-1 (D3 admission).** A record of another detached run is skipped ONLY when its owning bridge
is proven alive: `Recovery.concurrent` now needs the live-writer verdict of `test_pending_active`
(`PendingCheck.writer_alive`: the bridge's pid + start time run on this host), an ordinary
lifecycle state (`reserved|launching|running|committing`), no `survivors`, no `unverified`, no
`kill_unconfirmed`, a member-style name and no `parent_pid`. Every other active record (a dead
bridge with a live survivor, unverified descendants, an unconfirmed kill) blocks a detached run
exactly as before wave 9. Test: `recovery::a_detached_run_admits_only_records_of_a_live_owning_bridge`.

**F66-2 (reservation).** Under the task lock a concurrent detached run allocates its numbers
(`next_numbers` errors now refuse) and writes its reservation with `persist_reservation`: the atomic
write (file synced, then renamed), a refusal to overwrite an existing reservation file, and a
read-back of `n`/`nn`; any failure removes the file and refuses with the lock still held ("nothing
was started"). Only a verified reservation lets the lock go. Test hook
`CODEX_CONSULT_TEST_RESERVE_FAIL_ONCE=<flag file>`. Test:
`reservation_tests::a_failed_reservation_refuses_and_the_next_run_gets_distinct_numbers`.

**F66-3 (hits are consultations).** The machine-wide record carries the run's `consult_id`
(`MachineEndpoint.consult_id`, optional, omitted when empty) and its synthetic consultation is
marked `machine`. `instant_429s` drops a machine row that names a ledger row's consultation, or - a
record written before 9b - a machine row of the same endpoint within 5 s of a ledger row's finish,
and counts two rows at the same instant once. Test: `plan::a_429_mirrored_in_machine_health_counts_once`.

**F66-4 (Unicode hint).** `next_cap_reset` / `split_clock` accept ASCII only, so a hint such as
`daily:00:00` + a CJK character + `abcd` is a normal validation error ("ext.c3.reset_hint must be
...") instead of a panic at a byte offset. Test: `roster::an_entry_declares_its_plan_cap_under_ext_c3`
(eleven malformed hints).

**F66-5 (compat switch).** The shims no longer set `C3_PLUGIN_COMPAT` on their process: a function
`Invoke-C3Compat` sets it for the one `c3` child and restores the previous value in a `finally` on
every exit path. Test: `wave9_consilium::a_shim_run_leaves_no_compat_switch_in_the_session`.

**F66-6 (policy, documented).** The cap verdict is INFERRED exhaustion: the declared `reset_hint`
is the operator's statement about the plan; a transient pair on a capped plan costs the time to the
reset (cleared by a successful explicit run, or by editing the hint); without a hint the entry is out
for 24 h and then probed - eligible again, not evidence that capacity returned. Pinned (README roster
section too) by `plan::the_cap_policy_is_inferred_exhaustion_with_a_24_hour_probe`.

## 9c - the round-2 review (handoff 68: F68-1, F68-2)

**F68-1 (dedupe by identity, fail-closed).** 9b deduplicated the cap pair by time in two places:
two rows at one instant counted once even with different consult ids, and a legacy machine row (no
id) was dropped whenever an endpoint-matching ledger failure finished within 5 s. Both can HIDE a
real cap verdict - two independent consultations (another repository on the same machine file, or
two runs in one second) look like one - and the 5 s window was wrong because time proves nothing
about identity. Rule: two rows are one hit iff they carry the same non-empty consult id; rows with
different ids are distinct whatever their instants; a legacy row without an id is matched to a
ledger row only when EVERY identifying field it has agrees (the endpoint, the failure text - which
carries the provider's request id when it named one - and the instant to the second), otherwise it is
its own hit (a real mirrored legacy record written a second later counts twice; new records carry
the id). Test: `plan::hits_are_counted_by_consultation_identity_not_by_time`.

**F68-2 (portable fixture).** The unit test of the D3 admission no longer shells out to PowerShell:
its sleeping "bridge" is `ping 127.0.0.1 -n 60` on Windows and `sleep 60` elsewhere (as the liveness
tests do), and the recorded survivor's name is read from the liveness checker itself
(`process_info`), so the suite needs no PowerShell on Linux. Test:
`recovery::a_detached_run_admits_only_records_of_a_live_owning_bridge`.

## 9d - the acceptance review (handoff 72: F72-1, F72-2)

The legacy no-id fallback of 9c is REPLACED, not refined. **F72-2:** matching a no-id machine row to
a ledger row by endpoint + failure text + second compared `Record.message`, which is truncated to
100 characters - independent same-second failures whose request ids sit after the prefix merged into
one hit (hiding the cap) and genuine mirrors at different seconds split. **F72-1:** the 24 h age
bound on legacy rows ran only inside `update_machine_health` (retention), so an old no-id mirror
kept producing the cap outage until the configured reset if no successful update pruned it.
Rule: rows WITHOUT a consult id take no part in the cap pair at all - they keep the plain 10-minute
burst verdict exactly as before wave 9 - so the pair is built only from id-carrying rows
(`MachineEndpoint.consult_id`, the ledger's own `consult_id`), the identity rule is exact (same id =
one hit, different ids = two whatever their instants or texts). Wave 9d bounded the pair at read time
with an unconditional 24 h age filter in `endpoint_health` (`CAP_PAIR_MAX_AGE_MINUTES`); that bound was
wrong for a hinted entry whose declared reset is further out, and wave 9e (below) replaces it with the
per-entry cap-period rule - the read-time bound is still applied by every reader independent of
retention, but it is now the entry's current cap period, not a flat 24 h. Without a `reset_hint`
the entry is out until the pair is 24 h old (24 h after its first hit), then probed. Residual: the
machine rows written before 9b lack the id, so for the 24 h after an upgrade the cap verdict can only
come from new runs; that ends by itself. Two consequences GLM noted at round 5 (F74-1, F74-2), both by
design: every instant 429 of ONE consultation shares its id and counts once, so exhaustion seen only inside a
single run stays on the 10-minute burst; and the `until` fallback for a hint that `next_cap_reset` cannot
resolve (second hit + 24 h) is unreachable, because the roster validator accepts a `reset_hint` only when it
resolves at load. Tests: `plan::hits_are_counted_by_consultation_identity_not_by_time`
(a no-id legacy mirror -> one hit / the burst, never the cap verdict; two ids at one instant -> the
plan verdict; two id-carrying hits 10 min apart -> the plan verdict; two hits 25 h old -> none for an
unhinted entry, the cap verdict for a hinted entry whose reset is still ahead), and the 9b/F66-6
policy tests with id-carrying rows.

## 9e - the second acceptance pass (handoff 76: F76-1)

**F76-1 (blocker):** the 9d read-time bound was an unconditional 24 h age filter applied in
`endpoint_health` (`CAP_PAIR_MAX_AGE_MINUTES`). A hinted entry exhausted on 2026-10-14 and declaring a
reset of 2026-10-26 was wrongly reported "available" again two days later, because both hits aged out
of the 24 h window although the declared cap period had not reset. Rule: a hit is pair evidence only
while it lies inside the entry's CURRENT cap period - a hinted entry's hit counts iff the reset that
follows it has not passed (`next_cap_reset(hint, hit) > now`), an unhinted entry's hit counts iff it is
younger than 24 h at read time (the 9d rule, unchanged). `until` is unchanged (hinted ->
`next_cap_reset(hint, second)`, unhinted -> first hit + 24 h), and rows without a `consult_id` still
take no part. `endpoint_health` now retains id-carrying hits up to a generous fixed bound
(`CAP_PAIR_RETENTION_MINUTES`, 35 days - the longest monthly period) and `plan_cap_verdict` applies the
per-entry period rule, which keeps the verdict identical across every reader (providers table,
preflight, `plan_verdict`) independent of retention. A pair lying entirely before the last reset is no
evidence. Tests: the RC1 case (hinted stays out at 2026-10-16, available after 2026-10-26 16:00) and
the hinted/unhinted split of the 25 h-old pair in `plan::the_cap_policy_is_inferred_exhaustion_with_a_24_hour_probe`
and `plan::hits_are_counted_by_consultation_identity_not_by_time`.

## 9f - the third acceptance pass (handoff 78: F78-1, F78-2)

**F78-1 (blocker):** the detector fixed `second = hits[0]` (the newest in-period hit) and looked for a
partner within 60 min of IT only, so a later ISOLATED instant 429 (a third distinct consultation at
22:00Z after a 20:00/20:20Z pair, read at 22:11Z) hid the established pair and the exhausted entry read
available before its declared reset. Fix: `plan_cap_verdict` now scans all retained distinct-id
in-period hits newest-first and takes the newest QUALIFYING pair (any two within 60 min) for
`second`/`first`/`until`/`gap_min`. **F78-2 (blocker):** `update_machine_health` retention kept a row
only while `recent` (<= 24 h) or `still_out`, so id-carrying instant-429 evidence was pruned once its
ordinary burst `until` expired - a reader in ANOTHER repository (no ledger copies of its own) then lost
the pair before the declared reset, and the 35-day `CAP_PAIR_RETENTION_MINUTES` only filtered rows
already loaded. Fix: retention now keeps an id-carrying instant-429 row (`consult_id` non-empty, class
`quota`, kind `burst` - exactly what `endpoint_health` counts, since `machine_endpoint_consults` mirrors
it with `finished_at == when`) for `CAP_PAIR_RETENTION_MINUTES` regardless of `until`, and the
500-record cap (`cap_machine_endpoints`) evicts non-evidence rows first and the oldest evidence rows
only when none are left. The replay guard is unaffected (evidence keys merely persist longer). Tests:
the RC1 isolated-hit case in `plan::the_cap_policy_is_inferred_exhaustion_with_a_24_hour_probe`, and the
production-path RC2 (`health::machine_health_retention_keeps_cap_evidence_until_the_period_ends`) plus
the cap-ordering unit test `health::the_record_cap_evicts_cap_evidence_last`.
