Write in English.

# Handoff 33 - claude: C3 0.2.0, the compatibility release - acceptance

Date: 2026-10-09 (to run when the acceptance reviewer is back, 2026-10-14). Base commit: `0721045 (collab: the latest state commit at the time of writing)` (branch `main`;
code at 09b9d25 = version 0.2.0). The range under review is `4cd8d18..09b9d25` - the whole parity track since the
cloud session's Linux port; skip `.collab/`. Plan and record: `.collab/parity-0.6.1-2026-10-08/state.md` (decisions
P1-P8 and a dated log), the per-wave notes `docs/port/wave*.md`, the oracle `docs/port/rc2-oracle-2026-10-08.md` +
`rc2-triage-2026-10-08.md`, the verifications `docs/port/rc5-verification-2026-10-09.md`,
`rc3-live-matrix-2026-10-09.md`, `rc6-final-2026-10-09.md`, and `CHANGELOG.md` `[0.2.0]`.

## Question

Is C3 0.2.0 the compatibility release the plan P1 defined - the plugin's 0.6.1 behaviour, measured by the pinned
v0.6.1 harnesses through the shim and by live runs on this machine's subscriptions - and can it be tagged?
ACCEPT, HOLD (blockers by id) or ADVISE. You accepted wave 1 (handoff 06); your framing (02) shaped the plan; your
HOLDs on waves 2 and 2b (09, 11, 19) are implemented; while you were out, kimi :: k3 (14) and mimo :: mimo-v2.6-pro
(22-25, 29-32) reviewed the rest, held every wave once or twice, and every finding they filed is implemented
(the `-List` below). This is the first review of the WHOLE delta by the acceptance reviewer.

## What 0.2.0 contains (one line per wave; the finding ids closed)

- 1 / 1b / 1c: the time-only reset parse (5-min allowance, DST by UTC round trip, zone qualifiers; Samoa F06-1),
  E27/E28 (the Codex app's servers excluded from the machine-wide scan), `panel: light`, the re-read anchor
  (F02-6), the roster `plan` key with plan quota propagation and the plan wait across repositories (F02-7);
  F04-1..3.
- 2 / 2d / 2f / 2g: telemetry 0.6.1 - `consult_ref`, the saved rating `judge` and `rating_rev`, client_time = the
  mark's `when`, backfill, the ledger key order; the outbox with two locks and exact-line removal; the deletion
  transaction (pending/confirmed/cleaning) with the complaint inside the protocol; every outbound event
  re-serialised through the closed classes; the shared coordinator resolver; F02-1..4, F09-1..6, F14-1, F19-1..3.
- 2b / 2e / 2h / 2i: the pre-0.6 gaps of the RC2 triage (test-mode warning and child scrub, `raw_arg` for batch
  launchers, the health journal with `.bad`, kick/pending/store/member-steps/launch-mark hooks, `--explain`, the
  hook pointer, `--require` on a single run, role/topic validation, the shim fixes); F11-1..5 (the journal's
  error paths, the full roster for plan evaluation - beyond the plugin, plugin T11; the AGY tool-step stall
  allowance); the applied marks INSIDE the shared journal (F22-1, F29-1; beyond the plugin, plugin T14).
- 3a / 3c / 3e: recovery E1/E18/E19/E23/E25/E28 (`unverified[]`, `kill_unconfirmed`, the fail-closed re-check,
  both scans clean before a release); F23-1..5 and F30-1..3 (start times via NtQuerySystemInformation, the whole
  subtree through unreadable intermediates, the record written BEFORE the kill - beyond the plugin).
- 3b / 3d / 3f: the not-spooled count with per-producer files, the `.last` fold saving before deleting, legacy
  staging, the forgetting marker with ticks; F24-1..6, F31-1 (a lock-busy flush counts nothing).
- 4 / 4f / 4g: the `claude` engine, SPAWNED Claude Code (P4): roster `auth: subscription | api-key | endpoint`,
  the closed model table, the allow-listed child env, the strict tree check, the init proof, E12-E17, the
  quota mark, the panel seat; F25-1..4 and F32-1..2 (the endpoint launcher probed in the endpoint turn's
  environment without the token, the transcript location derived as Claude Code does and fail-closed - beyond
  the plugin, plugin T13).
- 5 / 6: `--brief-prefix`, the coordinator refusal/warning texts, the telemetry sender parity (429/400/413/403, the
  60 s flush and 8 s request deadlines, the owner record), the coordinator record fields and foreign ledger
  entries preserved verbatim, the providers header count, the agy/muse reviewer line, a detached sender after
  every commit; version 0.2.0, CHANGELOG, README.

Decisions you should judge: P7 (C3 keeps its own telemetry app id/root/outbox - two instances on a machine with
both bridges; 83-96 harness-telemetry checks fail by design), P8 (a test-mode-only path switch lets the fold/marker
checks run on the plugin paths), P4 (no native Messages API in this release).

## CURRENT invariants claimed

- Every `.collab` file is byte-compatible with the plugin's: the interchange (RC3 §6) had the plugin write n=1,
  C3 fork n=2, each rate the other's entry - 87 ledger keys in the same order, marks identical incl. `rating_rev`
  and `judge`, lists byte-identical, no warning either way; foreign entries survive C3 rewrites verbatim.
- Telemetry sends classes and numbers only; `consult_ref` is a pseudonymous key; the outbox never loses an event
  (locks, exact-line removal, the deletion transaction); a count is never lost or doubled across producers, folds,
  crashes (hooks 87/88) and forgets.
- Recovery is fail-closed: an unverified pid or an unconfirmed kill refuses the task until both scans are clean;
  the record is on disk before the kill.
- Keys are never created, printed, read back or committed; the claude endpoint token never reaches a probe, a
  log or a Debug output.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| RC5 | cargo gates + 22 harnesses | fd10f81 | 0 | 716 tests; 21 harnesses as expected | completed |
| RC3 live matrix | 14 live consultations, a 4-member panel, the interchange | f62ca02 | 0 | 14/14 usable (codex ZAI/mimo/byteplus, agy, muse, claude subscription, ZAI-claude + mimo-claude endpoints) | completed |
| RC6 final | cargo gates + 22 harnesses + a live smoke | 09b9d25 | 0 | 772 tests, clippy/fmt clean; all 22 harnesses at the expected counts, no reruns; live: ZAI codex 37 s usable and its event delivered by the detached sender within 10 s, claude subscription 7 s usable, a rating with rating_rev 1 and judge {anthropic, other, consult_coordinator}, the coordinator record with host_by/in_roster/unresolved | completed |

Harness numbers at RC6 (pass/fail): lock2 11/0, roster 125/0, panel 62/0, fixes 56/0, fixes28e 63/2, fixes28d 25/2, pending 26/0, detach 50/1, telemetry 47/96, claude 87/0, 0.3 229/0, engines 97/0, muse 73/1, companions 42/0, visibility 34/0 (then the harness crashes on its own `Invoke-EngineTurn` extraction), host 59/6, format 37/0, 3b 12/0, fixes26b 51/0, fixes27c 34/2, fixes28b 20/0, fixes28c 14/1 (`docs/port/rc6-final-2026-10-09.md`). The remaining failures are: by design P7 (telemetry 96),
source-grep shim artifacts (fixes28e RECORD x2, muse UNIT D2, fixes27c POINTER D15, detach CARRY), the harness's
own `Add-KillCheck`/`Invoke-EngineTurn` extraction (fixes28d stop, visibility crash), not applicable (host x6
Z Code/Codex-host rows, fixes27c ZCODE), fixes28c IDENTITY D8 (the forgetting marker's identity check reads the
plugin's marker path - P8 scope).

## Open findings

`-Task parity-0.6.1-2026-10-08 -List`: everything `implemented`; F02-5 `implemented`; F14-2 `wontfix` (parity:
the plugin matches providers case-sensitively); F02-8 note. Filed for the PLUGIN 0.6.2 from this track: T11, T12,
T13, T14.

## Questions

- **Q1.** Compatibility: a behaviour of plugin 0.6.1 a user would meet that 0.2.0 lacks or does differently
  without the doc saying so (the `docs/port/wave*.md` "differs from the plugin" lists are the claim)?
- **Q2.** The places C3 goes BEYOND the plugin (the full roster for plan evaluation, the applied marks in the
  journal, the record before the kill, the endpoint probe and transcript guard, the complaint in the deletion
  protocol): any that breaks interchange with a plugin on the same machine?
- **Q3.** P7/P8: acceptable for the release, with the harness-telemetry oracle replaced by C3's own tests?
- **Q4.** Verdict: ACCEPT (tag v0.2.0), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 900 words.
