# parity-0.6.1 - C3 to the plugin's 0.6.1 behaviour, and beyond (2026-10-08)

Goal (the operator, 2026-10-08 19:50): bring the Rust bridge to parity with claude-codex-consult 0.6.1 and, where
C3 can do more (the http engine, packs, the SurrealDB index, the router, MCP), do better. Coordinator: Claude
Fable 5.1 in Claude Code. Acceptance authority for releases: openai :: gpt-6-astra (as for the plugin).

## Baseline (gap recon, 2026-10-08 19:40)

C3 main 4cd8d18 (the cloud session's Linux port merged; verified live only through OpenRouter) implements the
plugin up to 0.5.0 wave 26b plus the 0.5.1 rating event (without the 0.6.1 keys). Absent from 0.6.0/0.6.1:

| # | plugin behaviour | C3 |
|---|---|---|
| A | time-only reset parse (today/tomorrow candidates, 5-min allowance, DST, UTC/GMT/Z and numeric offsets, other zone words decline) | absent (`c3-core/src/health.rs` parses month-name and ISO forms only) |
| B | E27/E28: the machine-wide codex scan excludes the Codex desktop app's servers (`app-server`, `exec-server`, `mcp-server`, `login`, `app`, `codex-computer-use*`, `--parent-pid` without `exec`), never a command line with the word `exec`, unreadable/ambiguous counted as codex | absent (`liveness/proc.rs:174-212`) |
| C | roster `panel: light` (joins light purposes; stands in on a weighty one only when no sibling of its label runs) | absent (`providers.rs:667`) |
| D | E4 re-read anchor: first line whole, 300-char cut, `(+n more lines)` | not checked |
| E | 0.6.1 telemetry: `consult_ref` per ledger entry (consultation + rating events), rating `judge {provider, model, source}` resolved at rating time and saved in the mark, `rating_rev` under the task lock, client_time = the mark's `when`, `telemetry_sent`, `-BackfillRatings` | absent (`telemetry/mod.rs:171`, `findings_tool/mod.rs:960-1121`); the mark IS written before `record_rating` already (F02-5) |
| F | recovery E1/E18/E19/E23/E25: `unverified[]` beside `survivors[]`, fail-closed re-check (unreadable command line = running), `kill_unconfirmed` released only after both scans, a kill with only unverified descendants keeps the record | partial (`confirm_tree_kill`, `kill_unconfirmed` exist; no unverified path) |
| G | E2/E3/E20/E24/E26: per-producer not-spooled files, the `.last` fold saving before deleting (`{name, bytes}`), legacy staging, the forgetting marker with start ticks | absent (one `spool.ndjson`) |
| H | the `claude` engine: `claude -p` with `--json-schema`, auth `subscription` / `api-key` / `endpoint {base_url, env_key, timeout_ms}` + `plan`, the closed model table (incl. `claude-haiku-5-5`), child-env allow-list, strict tree check, E12-E17 (killed-turn proof, `message.model` check, `rate_limit_event`, plan wait), coordinator rule, vendor classes `anthropic`/`minimax`, fake CLI + harness | absent |

Reverse (C3 beyond the plugin): the `http` engine (OpenRouter / any OpenAI-compatible endpoint, packs instead of
tools), context packs and `c3 explain`, the SurrealDB index, the router, the scoreboard, the MCP server, Linux.

## Plan (framed with Astra, handoff 02 - ADVISE, F02-1..F02-8)

Decisions (coordinator, 2026-10-08 20:20):
- P1. Two tracks. The COMPATIBILITY release (C3 0.2.0 = plugin 0.6.1 behaviour, measured by the pinned v0.6.1
  harnesses through the shim plus live runs) and a separately measured IMPROVEMENT track (native Messages API,
  SurrealDB-derived views, an outbox) - a passing harness, a native reply and a better review are three outcomes.
- P2. Waves of the compatibility track: 1 = A + B + C + D (running, branch `wave1-parity`); 1b = the roster
  `plan` key and the machine-wide running records / quota propagation / plan wait as SHARED infrastructure
  (F02-7; today the roster rejects `plan`, so a 0.6 plugin roster refuses every C3 run); 2 = telemetry: a
  durable spool (F02-1 lost-update race), closed vendor/model classifiers for reviewers and judges (F02-2), the
  forget-me retry identity (F02-3), consult_ref / saved judge / rating_rev / telemetry_sent / backfill, the
  telemetry shim; 3 = recovery F + G; 4 = H, the `claude` engine SPAWNED (all three auth modes, as the plugin).
  Each wave: one worker with tests, the pinned harnesses through the shim, then a diff-review by Astra.
- P3. Oracle: RC2 - the pinned v0.6.1 scripts dir, all 22 harnesses sequentially, every assertion mapped to
  "C3 executes" / "copied PowerShell helper" / "documentation" (F02-4), helper-only fixtures ported to Rust tests;
  runs after wave 1 merges. RC3 - the live matrix on this machine (codex, agy, muse, claude subscription,
  api-key, each endpoint plan; a mixed panel; alternating plugin/C3 writes on one task) before the release.
- P4. The endpoint route stays spawned Claude Code for parity; native Messages is an improvement candidate
  gated on RC4 (a three-route benchmark; the plugin's 3-6x measured codex vs Claude Code, not native - F02-8).
- P5. Files stay authoritative for shared recovery and health (the plugin reads them); SurrealDB stays derived.
- P6. Findings F02-1..F02-4, F02-7 are assigned to the waves above (not fixed in wave 1).

## Log

- 2026-10-08 19:50: task opened; build/test/shim recon of main 4cd8d18 on Windows running (worker); framing brief 01.
- 2026-10-08 20:20: Astra's framing (02) rated; plan P1-P6 above; wave 1 running (worker, worktree c3-wave1);
  C3 builds on Windows (522 tests, clippy clean, `-j 2`), shim verified on harness-lock2.
- 2026-10-08 22:50: wave 1 ACCEPTED by Astra (handoff 06; F04-1..3 closed, F06-1 minor Samoa-2012 fixture = follow-up in wave 3); wave 1b (plan key, c6ac728) merged 832aaf0 - verification of the merged main running; RC2 partial (its scripts dir broke mid-run), report pending.
- 2026-10-08 23:00: RC2 done (docs/port/rc2-oracle-2026-10-08.md, binary bd96c44): real C3 oracles = roster, panel, 0.3, engines, muse, format, fixes, detach, pending, companions, fixes26b/27c/28b/28c + the C3 parts of claude/telemetry/visibility; lock2 and 3b test the PowerShell library only; ~100 checks need a TELEMETRY SHIM (telemetry 48, fixes28e 24, fixes28d 25, detach 4); visibility/fixes28d crash because they extract functions from the real codex-consult.ps1 source; host hung once (30 min) on a shimmed -DryRun. Failures mapped: wave 2 = ledger field order (0.3 LEDGER, roster WALK/RATE, companions ROLE/RATE, engines/muse RUN, telemetry SPOOL/RATE); wave 3 = fixes28e RECORD/NOTSPOOLED/MARKER, fixes E28, fixes28d; wave 4 = claude DRYRUN/ENGINEEXE. NOT attributable (pre-0.6 gaps, need triage = wave 2b): agy, muse DRYRUN/RUN/PANEL, companions routing/roles, fixes27c/28b/28c, detach SINGLE/CARRY, panel SPEC, pending (e), format CONTR, roster TIMEONLY (pwsh 7 date artifact), 0.3 CFG.
- 2026-10-08 23:25: RC2 triage (docs/port/rc2-triage-2026-10-08.md): ~52 C3-gaps, ~21 shim artifacts, 4 harness-env. Wave 2b (after wave 2 merges) = the test-mode warning + CODEX_CONSULT_TEST_* scrub from children (~15 checks), the health journal at a failed commit + orphan replay + .bad, small compat bugs (kick during continuation, pending (e) outcome, store rename retry, -MaxModelSteps on a codex member, launch-mark hooks, STREAM D6), -Require on a single run and the role/topic slug validation, the hook second line + -Explain; shim fixes (optional -Task, -By, -CodexConfig whole, the plugin tree staged, the schema path). Decisions: claude wording = wave 4; the Z Code host checks are NOT applicable (C3 is Claude Code only - record as n/a in the shim note); the ledger fields (consult_ref, context_window, compactions, preview topics/role/format_retry) go with wave 2a.
- 2026-10-09 02:00: wave 2 (telemetry: 3d33a54 consult_ref/judge/rating_rev/backfill + ledger order; 136cbf8 the outbox + forget-me identity; b1ba064 closed classes + the telemetry shim) merged 3605395 - 576 tests; harness-telemetry 45/98 (81 of the 98 read the PLUGIN own spool/salt/instance: C3 keeps its own telemetry root <codex home>/c3/telemetry and app id c3 - the identity question goes to Astra, handoff 07); roster 124/1, 0.3 227/2, format 37/0, fixes28c 13/2. Astra diff-review of wave 2 running (5bdfbe48). Wave 2b (the triage gaps, launcher quoting via raw_arg, shim fixes) running on branch wave2b-compat.
- 2026-10-09 03:50: Astra on wave 2 (handoff 09): HOLD F09-1..F09-5 (+F09-6 minor). P7 (decided): C3 KEEPS its own telemetry app id, root and outbox; the plugin harness-telemetry checks that read the plugin paths are adapted as fixtures later (storage/app differ, semantics asserted); a shared logical identity across bridges is not promised - the owner of a consultation telemetry is the bridge that recorded it. Wave 2d (F09 fixes) starts now in parallel with 2b.
- 2026-10-09 04:10: wave 2b (2ebddcb, 460f00b, 7dace6e) merged 1fa1ba7 - 600 tests; 13 baselined harnesses 806/59 -> 853/12, host 55/10 (no hang). Astra diff-review of 2b running (handoff 10). Wave 2d (F09 fixes) running.
- 2026-10-09 04:20: Astra on wave 2b (handoff 11): HOLD F11-1..F11-3 (+F11-4 minor, F11-5 major agy stall) - wave 2e starts now; F11-3 is INHERITED from the plugin (the plugin filters the roster before plan evaluation too) -> file it for the plugin 0.6.2 as well. Wave 3a (recovery) running; 2d (F09) running.
- 2026-10-09 05:00: wave 2d (F09-1..6, bc8d605) merged 1c8c6f8 (with 2b; conflict-free, verification of the merged main running); Astra round 2 on wave 2 (handoff 12) running. Branches live: wave2e-fixes (F11), wave3a-recovery.
- 2026-10-09 05:20: Astra out until 08:16 (second window hit of the night); Kimi k3 second opinion on wave 2d (handoff 14): HOLD F14-1 blocker (local-only forget writes no transaction record), F14-2 minor (case-sensitive provider match) -> wave 2f fix now; Astra confirms 2d+2f after 08:16.
- 2026-10-09 05:45: wave 2f (F14-1 fixed; F14-2 wontfix - the plugin matches providers case-sensitively, the doc corrected) merged 5c9f8a0; 614 tests on the branch.
- 2026-10-09 06:20: merged main 1c8c6f8 verified (610 tests, clippy/fmt clean; 0.3 229/0, companions 42/0, panel 62/0, roster 124/1 FILE, telemetry 46/97 by design P7, host 55/10). Host PREFIX D6 x2 (--brief-prefix never ported) and REFUSE D3 (coordinator refusal wording) stay open -> port --brief-prefix and the refusal text in wave 4 or a small 2g. Harness runs need PSModulePath reset when inherited from pwsh 7 (Get-FileHash).
- 2026-10-09 06:40: wave 2e (F11-1..5, 4b51226) merged 4597ec5 (611 tests on the branch); awaiting Astra (08:16) for: wave 2 round 2 (2d+2f) and wave 2b round 2 (2e). Running: 3a, 3b.
- 2026-10-09 07:00: wave 3a (recovery E1/E18/E19/E23/E25/E28 + Samoa F06-1; 954a5e6, 1081079) merged 8ff53c4 - 616 tests on the branch; fixes28e 31/34 -> 48/17 (13 NOTSPOOLED + 2 MARKER = 3b, 2 source-grep shim artifacts), fixes 56/0. Pending Astra reviews: wave 2 r2 (15), wave 2b r2 (16), wave 3a (to write). 3b on its harnesses.
- 2026-10-09 07:30: wave 3b (not-spooled per-producer files, the .last fold saving before deleting, legacy staging, the ticks marker; da7bfdd) merged d71ae01 - 631 tests on the branch; fixes28e NOTSPOOLED+MARKER green (with 3a: expect ~63/2). P8 (decided): the test-mode-only switch C3_TEST_TELEMETRY_PLUGIN_HOME (set by the shims) points ONLY the not-spooled files, .last and the forgetting marker at the plugin home so the pinned harness can be the oracle for the fold logic; production and P7 unchanged. Still open small: the run warning wording "- counted" vs the plugin "- dropped" (orchestrate.rs) -> wave 4 or 2g; the sender 429/400/413 handling (older gap).
- 2026-10-09 08:00: merged main d71ae01 verified: 658 tests, clippy/fmt clean; fixes28e 63/2 (source-grep shim artifacts), fixes28d 25/2, fixes 56/0, pending 26/0, panel 62/0, roster 124/1 (FILE = wave 4), telemetry 47/96 (P7), companions 42/0, engines 95/2 (wave 4 wording), 0.3 229/0. Wave 4 restarted after the first worker overflowed its context (31 uncommitted files kept in the worktree).
- 2026-10-09 08:35: Astra wave 2 round 2 (handoff 19): HOLD F19-1..F19-3 -> wave 2g now. Astra then hit the WEEKLY cap: out until Wed 2026-10-14 11:37 (5 days). Per the operator rule of 2026-09-25 (new waves go ahead with Kimi & co without waiting for Astra) the interim diff-reviews go to kimi :: k3 (chained: 2b r2, 3a, 3b); the RELEASE acceptance waits for Astra or the operator decision.
- 2026-10-09 09:00: wave 2g (F19-1..3, 09a64f2) merged 09a64f2 - 662 tests on the branch. Kimi out until 13:31 (5-hour window); MiMo chain running (2b r2, 3a, 3b).
- 2026-10-09 09:50: wave 4 (the claude engine, SPAWNED: 88ce0b5 4a, 1ca23d6 4b "- dropped", 1185fac merge of main, 115f015 4d test, 1fdb8bd 4e doc) merged fast-forward -> main 1fdb8bd. harness-claude 17/5 -> 87/0, roster 125/0, engines 97/0, muse 73/1 (UNIT D2 shim), panel 62/0. LIVE smoke on the real Claude Code subscription (2.1.294): dry run clean, one chore consultation usable in 6.3 s, model claude-haiku-5-5, reviewer record {anthropic, haiku, claude, claude-cli 2.1.294, subscription}. MiMo HOLDs on 2b r2 (F22-1 journal replay after eviction) and 3a (F23-1..5: E19 fail-open paths, the record written only at the end) -> waves 2h and 3c running. Pending (task lock held by the MiMo chain): ratings n=10, n=11, the marks F19-1..3, the wave 4 review brief 24.
- 2026-10-09 10:10: MiMo on wave 3b (reply 24-codex-...-mimo; the brief for wave 4 is also numbered 24 - a numbering collision, both kept): HOLD F24-1..F24-6 -> wave 3d now. Ratings n=10-12 and marks F19-1..3 done. Wave 4 review by MiMo launched (brief 24-claude).
- 2026-10-09 10:40: wave 2h (F22-1: an applied-key list beside the journal, c5d8b28) merged f46410b - 665 tests on the branch. Running: 3c (F23), 3d (F24), MiMo review of wave 4 (bb3c3407). Pending (task lock): re-rate n=10/n=11 as VOID, rate n=12-14, mark F22-1.
- 2026-10-09 10:20: MiMo on wave 4 (handoff 25): HOLD F25-1..F25-4 -> wave 4f now. Ratings n=10/11 re-rated VOID (no), n=12-15 yes; F22-1 implemented.
- 2026-10-09 11:00: wave 3c (F23-1..5, b649e96) merged 6f7a8f7 - 669 tests on the branch; fixes28e 63/2, fixes 56/0 (a first run 53/3 while another session ran Codex: the scan rule correctly saw its processes). Running: 3d (F24), 4f (F25).
- 2026-10-09 11:20: wave 3c merged 2ecf52c (one conflict in orchestrate.rs: the claude fields of wave 4 + the kill-site fields of 3c, combined by hand; c3 builds; the two kill_record_durable tests failed once on a binary another worktree had overwritten in the shared target, pass on a fresh main build). Running: 3d (F24), 4f (F25).
- 2026-10-09 11:35: wave 3d (F24-1..6, e9f3127) merged 46ac66e - 699 tests on the branch. Running: 4f (F25). Next after 4f: verification of main, then second rounds (2b r3? no: 2h closed F22-1; 3a r2 = 3c; 3b r2 = 3d; wave 4 r2 = 4f) with MiMo, the RC3 live matrix, the release prep.
- 2026-10-09 11:40: wave 4f (F25-1..4, fe2fea9) merged fd10f81 - 696 tests on the branch; claude 87/0, roster 125/0. Note: the plugin 0.6.1 itself has the F25-1/F25-2 gaps (endpoint mode never runs auth status, so no launcher check and no projectsDirectory guard) -> plugin TECH_DEBT T13.
- 2026-10-09 12:20: MiMo second rounds: 2b r3 HOLD F29-1 (-> 2i running), 3a r2 HOLD F30-1..3 (-> 3e running), 3b r2 HOLD F31-1 (-> 3f running), 4 r2 HOLD F32-1..2 (-> 4g now). Wave 5 (leftovers + 0.2.0 bump) running; the full-suite verification of fd10f81 at 12/22. Ratings n=16-19 done; F25-1..4 implemented.
- 2026-10-09 13:30: wave 4g (F32-1, F32-2, f62ca02) merged f62ca02. Still running: 5, 2i, 3e, 3f; the verification of fd10f81 at 21/22.
- 2026-10-09 13:50: RC5 verification of fd10f81 (docs/port/rc5-verification-2026-10-09.md): 716 tests, clippy/fmt clean; 21 harnesses as expected (telemetry unmeasured: the 20-min cap hit while waiting on the lock; host 56/9; fixes and fixes26b flaky once under load, green on rerun). Final verification (RC6) after waves 5, 2i, 3e, 3f merge; then the RC3 live matrix.
- 2026-10-09 14:00: wave 3f (F31-1, 73ed5fe) merged b683dc6. Disk C hit 7 MB free during a link: the worker deleted idle 9eck/xelth target/debug/incremental (5.9 GB) under the standing permission. Running: 5 (first commit 8c01b7e), 2i, 3e, RC3 live matrix.
- 2026-10-09 14:30: wave 2i (F29-1, 2f9edda: applied marks inside the journal, proven against the plugin real updater) merged 3b1df07. Plugin TECH_DEBT T14 to file: the plugin applies every line with an endpoint, marked or not, and marks nothing it applies; its in-place tail rewrite can tear the journal. Running: 5, 3e, RC3.
- 2026-10-09 14:45: RC3 live matrix PASSED (docs/port/rc3-live-matrix-2026-10-09.md): 14/14 usable - codex ZAI 51 s, mimo 310 s, byteplus 42 s; agy 208 s (tree clean); muse 49 s; claude subscription 8 s, ZAI-claude endpoint 9 s (dry run: token absent), mimo-claude 16 s; a 4-member panel 677 s (plan zai + the claude group serialised by the ledger times); interchange plugin n=1 / C3 fork n=2 / cross ratings: 87 ledger keys in the same order, marks identical, lists byte-identical, no warnings either way. Five differences -> wave 6 (after 5 and 3e merge): the coordinator record lacks host_by/in_roster/unresolved and C3 rewrites of a plugin entry drop its nulls and reorder host_by (fix: write the fields, preserve foreign entries verbatim); the providers header counts machine-wide health records (fix: this repo only); the reviewer line for agy/muse (engine agy (<launcher>)); telemetry sent at the start of the next run (fix: a detached sender after the commit as the plugin); identity = P7.
- 2026-10-09 15:10: wave 5 (--brief-prefix, the coordinator refusal/warning texts, the telemetry sender parity 429/400/413/403 + the owner record, version 0.2.0 + CHANGELOG + README; 8c01b7e, f00a823) merged 9bc502a - 737 tests on the branch; host 55/10 -> 59/6 (the rest n/a). Running: 3e. Next: wave 6 (RC3 differences), RC6 final verification, the acceptance brief, pause until Wed.
- 2026-10-09 15:30: wave 3e (F30-1..3, 9ee3a7b) merged 7a4d882 - 728 tests on the branch; the record is written BEFORE the kill (beyond the plugin). All review findings now implemented. Disk: cargo clean of the workspace crates + incremental removed before wave 6. Next: wave 6 (RC3 differences), RC6 final verification, the acceptance brief.
- 2026-10-09 16:30: wave 6 (the four RC3 differences, 09b9d25) merged 09b9d25 - 772 tests on the branch; harnesses unchanged (SEND x2 stay red only because the consult shim forces telemetry off and app_id is c3 - P7). Next: RC6 final verification of main, the acceptance brief, pause until Wed.

## Paused for the acceptance (2026-10-09 18:10)

C3 0.2.0 = main 09b9d25 (code). RC6 final: 772 tests, clippy/fmt clean, all 22 harnesses at the expected counts,
the live smoke clean (the detached sender delivered within 10 s; coordinator record complete). Every finding of
every review implemented (F14-2 wontfix = parity). The acceptance brief is `handoffs/33-claude-0.2.0-acceptance.md`.

TO RUN when openai :: gpt-6-astra is back (Wed 2026-10-14 11:37; check `codex-providers.ps1 -Short`), from ~/C3:

    CODEX_CONSULT_ROSTER=~/.codex/codex-consult-roster-0.6.json CODEX_CONSULT_COORDINATOR="anthropic :: claude-fable-5-1" \n    powershell -NoProfile -ExecutionPolicy Bypass -File <plugin>/scripts/codex-consult.ps1 -Task parity-0.6.1-2026-10-08 \n    -Provider openai -Mode fork -Purpose acceptance -Range 4cd8d18..09b9d25 \n    -Brief .collab/parity-0.6.1-2026-10-08/handoffs/33-claude-0.2.0-acceptance.md -ReplyName 0.2.0-acceptance -Detach

Then: rate it, implement blockers if any (a fresh worker per wave, the machine rules of the log), re-run RC6 on the
fixed main, tag `v0.2.0` on ACCEPT, push the tag. Open small items: the operator roster-0.6 lacks a claude
`auth: subscription` entry (add `{"provider":"anthropic","engine":"claude","model":"claude-haiku-5-5","auth":"subscription"}`
as a chore reviewer); one c3-cli compiler warning (the German linker note, `linker_messages`); the improvement track
(RC4 benchmark for a native Messages route, SurrealDB-derived views) is NOT started.

Idle since 2026-10-09 18:15; idle wake 2 at 19:15 - the handover is this section, the 30-minute wake removed. Resume on Wed 2026-10-14 after 11:37 with a fresh session from this file (the command above).

2026-10-13 13:36 - the operator asked to finish now; a one-word probe of openai :: gpt-6-astra answered, so the acceptance (handoff 33) was launched with -SkipPreflight past the stale weekly-cap record: n=20 FAILED after 186 s of reading - the 5-hour window ("try again at 6:32 PM"); partial in handoffs/34-...partial.md, thread 01a13509-4ed9-7683-8305-9856ed1db1aa. Resume at 18:36: `-Mode resume -Thread 01a13509-4ed9-7683-8305-9856ed1db1aa -Purpose acceptance -ReplyName 0.2.0-acceptance -Range 4cd8d18..09b9d25 -SkipPreflight -Prompt "finish your review"` (-Detach). Not rated (no reply; plugin T12).

2026-10-13 14:05 - why n=20 died: Codex rollouts record rate_limits per turn - the 5-hour window went 0 -> 96 % inside this one run (parent thread 13.9M input tokens, 12.1M cached, 20 tool calls of ~700k context; three Codex sub-agents (feature multi_agent) replayed 1.1-1.4M each); weekly window 15 % used, resets 2026-10-20 13:32. The diff of the range is 2.2 MB. Retry at 18:36 as a NEW thread, `-CodexConfig features.multi_agent=false`, with the reading budget now in brief 33. Plugin follow-up: quota awareness from the rollout rate_limits (T15).

2026-10-13 17:54 - MY MISTAKE: I read the 17:43 wake as 18:43 and launched the acceptance at 17:53 with -SkipPreflight past the new bridge's correct refusal (5-hour window 99 % until 18:32): n=21 (handoff 35) refused by the server in 7.6 s, no quota spent. The one-shot wake is re-armed for 18:37. Wave 30 of the plugin (quota windows) is merged in main a162b8d: the providers line now reads "unavailable (5-hour window 99 % used, resets 2026-10-13T18:32:16+02:00)" before any run.

2026-10-13 18:46 - ACCEPTANCE ROUND 1 (n=22, handoff 36) through plugin wave 30: usable in 442 s, 13 calls, max 123k context, 1.10M input (0.97M cached), sub-agents 0, the 5-hour window 0 -> 23 % (weekly 20 %). The quota guard WORKS (vs 96 % in 186 s on n=20). Verdict HOLD: F36-1 blocker (repo_relative slices at a byte length of an unrelated prefix - UTF-8 boundary panic on a Unicode CLAUDE_CONFIG_DIR), F36-2 major (the derived transcript guard skips the NFC normalisation wave4-claude.md attributes to Claude Code); re-raised priors: F30-2/F23-3 failure path (a failed pre-kill write still kills), F29-1 (plugin 0.6.1 replays - record as unsupported), F25-1/F32-1/F32-2 documented unsupported cases. RC1-RC4 in the reply. Next: wave 7 (a fresh worker) then round 2 with Astra (window allows: 23 %).

2026-10-13 21:30 - ROUND 2 (n=23, handoff 38): 383 s, 10 calls, 0.64M input, window 23 -> 44 %. HOLD on F38-1 only (F36-1/F36-2 fixed): when BOTH pre-kill writes fail and the bridge dies after killing an unrecorded intermediate, the recovery can release the task while a non-codex helper survives (KillRecord::Lost lives in memory only). Decision: fail closed - NO KILL WITHOUT A RECORD (the tree is left alive as living evidence, the run fails naming the pid, the next turn is refused from the launch record while the child lives) plus a third record location in the codex home before any kill; an external death of the ancestors with a generic helper stays a documented limit shared with the plugin. Wave 7b worker next, then round 3.

2026-10-13 22:12 - ROUND 3 (n=24, handoff 40): 341 s, 9 calls, 0.54M input, window 44 -> 61 %. HOLD: F40-1 blocker (pending_paths treats a read_dir/iteration error of <codex home>/c3/pending-kill as "no copies" - must be an inspection failure that refuses), F40-2 minor (a kick whose record is written nowhere still acknowledges "stopped"). Wave 7c worker next; round 4 after (window resets 23:37).

## Planned after the tag: worker telemetry (the operator's request, 2026-10-13 22:40)

Goal: know which worker tier (opus / sonnet / haiku / other) did what kind of work and how it ended, across installs
(users on an Opus-max or a Sonnet-max plan), WITHOUT spending the coordinator's tokens. Design:
- The signal source is the host's hook, not the model: Claude Code runs `SubagentStop` (and `SubagentStart`) hooks
  as shell commands with a JSON on stdin (session, transcript paths, the agent id - to verify: `agent_transcript_path`).
  C3 ships `c3 hook subagent-stop`; the plugin ships the PowerShell mirror in its hooks.json. Zero model tokens.
- From the subagent's transcript the hook computes CLASSES AND NUMBERS only: tier (the plugin's agent names
  opus-worker/sonnet-worker/haiku-worker, else `other`), the actual model (closed list of published names, else
  `other`), outcome (`completed`, `error:rate_limit`, `error:context` (prompt too long), `error:other`, `stopped`),
  duration bucket, input/output token buckets, tool-use count bucket, tool-error count bucket, edits bucket
  (Edit/Write calls -> `read-only` vs `writes`), whether a verification command ran (cargo test / pytest / run-all
  patterns -> `verified`), resumes (SendMessage continuations of the same agent). Never a prompt, a path, a name.
- Optional judgment at ~50 tokens: `c3 worker rate <agent id> yes|partly|no --note` (mirrors `-Rate` for reviewers);
  the event carries the mark like the reviewer rating (`rating_rev`, judge).
- One event per subagent end to the intake (event class `worker`); the intake (xelth.com crates/telemetry) gets the
  schema and a public table "workers": per tier x outcome counts, error share, median duration/tokens, the rating
  share - so a user can see whether e.g. sonnet-worker finishes verification waves as often as opus-worker.
- Cost: hook CPU only; the rating is optional. Privacy as the existing contract (closed classes, consult_ref-style
  pseudonymous keys). Order: after v0.2.0 - C3 wave "worker telemetry", the plugin mirror, the intake + page.

2026-10-13 22:50 - wave 7c merged (98a8742; F40-1, F40-2 implemented; 793 tests). Round 4 brief 41 launched (window ~61 % before; -Budget 85; resets 23:37).

2026-10-13 23:00 - ROUND 4 (n=25, handoff 42): 252 s, 9 calls, 0.44M input, window 61 -> 76 %. HOLD: F42-1 blocker (read_pending_file uses Path::exists - a metadata error on a copy reads as absent). Wave 7d: fix it AND sweep every error-to-absence conversion in the recovery readers so the chain ends. Round 5 after the 23:37 window reset.

BEFORE IMPLEMENTING the two planned waves above (worker telemetry; wave 31 "watch" - event-driven wakes) the operator wants a CONSILIUM first (2026-10-13 23:05): a framing panel (`-Panel -Purpose framing`, the full roster incl. Astra, Kimi, MiMo, GLM, Gemini, Muse) on one brief per topic - the design as written here, the open questions (what the SubagentStop hook really carries; what to count so a Sonnet-max and an Opus-max user can compare; the watch deadline rules and the cache TTL) - and the design is revised on the panel's answers before any worker starts.

2026-10-13 23:38 - wave 7d merged (05c4b9f; F42-1 implemented; the sweep 110/19/91; 795 tests). Round 5 brief 43 after the 23:37:59 window reset.

2026-10-13 23:45 - ROUND 5 (n=26, handoff 44): ACCEPT. 229 s, 9 calls, 0.55M input, window 0 -> 15 % (fresh). Five rounds today through the quota-aware bridge: 23+21+17+15+15 % of windows, 50 calls, no sub-agents. Before the tag: RC7 = the full RC6 procedure on 05c4b9f (cargo gates, 22 harnesses, live smoke) by a worker; tag v0.2.0 on 05c4b9f when green.

Worker telemetry, additions (the operator, 2026-10-14 00:00): (1) the worker's REASONING EFFORT (low/medium/high/xhigh/max) is a dimension next to the tier and the model - the hook must find it (the agent definition's `effort:` frontmatter; an explicit `effort` override of the Agent call may not be in the subagent transcript - an open question for the consilium; fallback: the coordinator's rate command carries `--effort`); the optimisation question is then "per task class, the cheapest tier x effort with a completion share above X". (2) Nobody watches the site or the telemetry today: the xelth.com assistant (crates/server/src/chat.rs C3_PROMPT) is a STATIC prompt and reads no stats; no scheduled job or agent exists. Plan: the intake computes aggregated tables nightly (reviewers: usefulness; workers: tier x effort x task class -> completion, error share, median tokens/duration, rating share) and a derived recommendation table; the C3 page shows them and the assistant's prompt gets the latest table injected at request time - no new agent needed; an autonomous weekly "optimiser" note is a consilium question, not a given. Note: the intake already has an `agent_report` event (model_name, agent_role, task_scope free text) from an older project - the worker event must NOT reuse it (free text breaks the closed-class contract).

2026-10-14 02:00 - RC7 green on 3e9473e (795 tests, clippy/fmt clean, 22 harnesses at the rc6 counts on the pinned v0.6.1 tests - the plugin's HEAD tests carry wave 30 and are not the oracle; live smoke ZAI 15.8 s + claude haiku 6 s, telemetry delivered). TAGGED v0.2.0.
