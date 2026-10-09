Write in English.

# Handoff 10 - claude: wave 2b of the compatibility track (the pre-0.6 parity gaps) - diff review

Date: 2026-10-09. Base commit: `1fa1ba7` (branch `main`; the range under review is `0d78ec0..1fa1ba7` - wave 2b in
three commits 2ebddcb / 460f00b / 7dace6e; skip `.collab/`). Independent of wave 2's telemetry (your HOLD 09 is
being fixed in parallel on `wave2d-fixes`, not in this range).

## Question

The RC2 triage (`docs/port/rc2-triage-2026-10-08.md`) found ~52 plugin behaviours of 0.5.x that C3 claimed
but lacked. Wave 2b ports them (`docs/port/wave2b-compat.md`). Faithful, and anything wrong for the
compatibility release? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta

- **2ebddcb:** the test-mode warning `test mode is ON: test hooks are honoured` in run/dry-run warnings and on
  the detach foreground (`c3-core/src/test_hooks.rs`); `CODEX_CONSULT_TEST_*` scrubbed from every engine child
  and launcher probe (`engines/mod.rs`); plain arguments go to a batch launcher through `raw_arg`
  (`engines/subprocess.rs`: no more `-c "key=value"` / `"-p="`; arguments that are not plain tokens keep Rust's
  batch escaping - `%` cannot expand, a line break is refused, a backslash before a quote is doubled); the
  machine-wide health journal: the record is kept in the journal at a failed commit, orphan replay, `.bad`
  handling, the `FAIL_FIRST` hook (`c3-core/src/health.rs`); health written before the lock, journaled inside it,
  retried after it (`orchestrate.rs`); agy/muse ledger `warnings[]` and handoff carry the run warnings.
- **460f00b:** the store rename retried 8 x 250 ms (`store.rs`); open tool calls tracked by key with labels so a
  stall names the open call, bound 2 x stall (`engine.rs`, the engines); a kick during the timeout continuation
  delivered; the registration-failure outcome kept (not overwritten by `codex exit 1`); a codex panel member
  drops `-MaxModelSteps`; the launch-mark/pause hooks `CODEX_CONSULT_TEST_MEMBER_LAUNCH_MARK` / `_PAUSE_MS`;
  `--explain coordinate|consult|providers` (`consult/explain.rs`; falls back to C3's own `skills/` dir); a
  missing `--task` refused by c3 with the plugin's text; the hook's second (pointer) line (`cli/hook.rs`,
  `plugin/hooks/c3-hook.{ps1,sh}`; the default pointer command is `"<c3>" consult --explain coordinate`).
- **7dace6e:** `--require` on a single run (exit 5 with who/why/when); `--role`/`--roles`/`--topic` lower-cased,
  slug-checked, refused with the plugin's texts; the dry-run `topics`/`role`/`required` lines; the plugin-root
  fallback (an unknown role is tolerated when no plugin root exists; a safety problem refuses);
  `C3_SCHEMA_FILE` honoured only when byte-identical to the embedded schema; shim fixes (`-Task` optional, `-By`,
  `-CodexConfig` whole, pre-escaped embedded quotes for Windows PowerShell 5.1), the staging recipe with the
  plugin tree, the Z Code host checks recorded as not applicable (C3 is Claude Code only).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt per commit | `cargo test --workspace -j 2` etc. | 2ebddcb, 460f00b, 7dace6e | 0 | 587, 598, 600 (was 576); clean | completed |
| 14 harnesses (shim v0.6.1, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | 7dace6e | - | 13 with a baseline: 806/59 -> 853/12; host no longer hangs: 55/10 | completed |
| per harness | | | | fixes28b 20/0, fixes28c 14/1, fixes27c 34/2, fixes26b 51/0, pending 26/0, detach 50/1, panel 62/0, companions 42/0, engines 95/2, muse 69/5, 0.3 229/0, format 37/0, roster 124/1 | completed |

Remaining failures: wave 4 (engines ROSTER/DRYRUN, muse ROSTER/DRYRUN/ENGINEEXE/PANEL, roster FILE - the claude
engine), wave 3 (fixes28c IDENTITY D8), shim artifacts (fixes27c POINTER D15, muse UNIT D2 - regexes over the
script source; detach CARRY - C3's args are JSON, not CLIXML), not applicable (fixes27c ZCODE D20/D21; host WARN
D3 host-codex, WARN 27b, ENV x3), still open (host REFUSE D3 - the coordinator parse refusal in C3's own wording;
host WARN D3 real run / F04-10 - C3 does not resolve a coordinator label's model or engine, wave 27c D9 - this is
also F09-6's resolver, being fixed in 2d; host PREFIX x2 - `-BriefPrefix` never ported).

## Questions

- **Q1.** The health journal and the write order (before the lock, journaled inside, retried after): a path that
  loses a record or replays one twice, versus the plugin's?
- **Q2.** `raw_arg` and the batch escaping: an argument the plugin passes that C3 now refuses or alters (the `%`,
  line-break and backslash-quote cases)?
- **Q3.** The still-open items (REFUSE D3 wording, `-BriefPrefix`, the label resolver): any of them a blocker for
  the compatibility release, or acceptable as a documented difference / a later wave?
- **Q4.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 600 words.
