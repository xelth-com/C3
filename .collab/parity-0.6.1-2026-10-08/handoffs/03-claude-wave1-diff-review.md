Write in English.

# Handoff 03 - claude: wave 1 of the compatibility track (A, B, C, D) - diff review

Date: 2026-10-08. Base commit: `bd96c44` (branch `main`; the range under review is `4cd8d18..bd96c44` - wave 1,
commit 42af7a9 merged; skip `.collab/`). Plan: `state.md` P1-P6 (your framing 02).

## Question

Wave 1 ports four behaviours of the plugin (v0.6.1) into C3. Is it a faithful port, and is anything in it wrong
for the compatibility release? ACCEPT, HOLD (blockers by id) or ADVISE. (Waves 1b-4 and F02-1..F02-4, F02-7 are
not in this range.)

## Delta

- **A - the time-only reset** (`crates/c3-core/src/health.rs`, `retry_after_in`): the clock time on the day
  before, of and after the reference in the reference zone, earliest candidate not before reference - 5 min;
  both instants in a repeated DST hour; a spring-gap time = the first valid minute after it; `UTC`/`GMT`/`Z` or a
  COMPLETE numeric offset honoured, any other zone word or malformed qualifier = no parse. Failures are parsed at
  the consult clock (`CODEX_CONSULT_NOW` in test mode) in the machine's local zone (`consult/orchestrate.rs`).
  Two extras: the dated forms follow the zone's DST rules at write time; the month-name regex has the plugin's
  word boundary after the day. The 69 plugin samples (harness-roster UNIT + TIMEONLY, incl. the 17 qualifier
  cases) are Rust tests; Berlin by IANA id (`chrono-tz`, dev-dependency).
- **B - E27/E28** (`crates/c3/src/liveness/proc.rs`, `pending.rs`): the machine-wide scan excludes a codex-named
  process running `app-server`, `exec-server`, `mcp-server`, `login`, `app`, a `codex-computer-use*` binary, or
  `--parent-pid` without `exec`, naming them (`excluded: pid N codex.exe [codex app-server]`, at most six); a
  command line holding the word `exec`, an unreadable one, or ambiguous quoting still counts as codex; the scan
  reads the command lines of codex-named processes; `CODEX_CONSULT_TEST_CMDLINE_UNREADABLE` honoured.
- **C - `panel: light`** (`c3-core/src/roster.rs`, `providers.rs`, `panel/{run,plan}.rs`): joins on the light
  purposes; on a weighty one held back, stands in only when no other entry of its label runs (`stands in for #1
  (...)` / `stands in (no other entry of label X)`); `--require` lifts the gate; the context skip runs before
  the light gate.
- **D - the re-read anchor** (`consult/prompt.rs`): C3 had no re-read line; D11 and D7 ported with E4 - a
  reviewer with a context window gets, last before the consultation id, the brief path or the ask's first line
  (cut at 300 characters, counted as characters not UTF-16 units) plus `(+n more lines)`.
- Docs: `docs/port/wave1-parity.md`, `docs/port/harness-shim.md` section 4 (the v0.6.1 scripts dir needs
  `codex-consult-detached.ps1`).

## Known differences from the plugin (documented in wave1-parity.md)

Zone source `chrono::Local` (tests: Berlin by IANA id, not `W. Europe Standard Time`); chrono's Windows local
zone treats 03:00 on the fall-back day as ambiguous where .NET does not; a comma-separated `CODEX_CONSULT_NOW`
uses the first token; the bare-pid re-check still lacks the plugin's wider E19 fail-closed rules (wave 3).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests | `cargo test --workspace -j 2` | 42af7a9 | 0 | 535 passed (522 + 13) | completed |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 42af7a9 | 0 | clean | completed |
| harness-roster (shim, v0.6.1) | `run-all.ps1 -ScriptsDir <v0.6.1 shim dir> -Only harness-roster` | 42af7a9 | 1 | 121/4 (main: 115/10): TIMEONLY x4 and WEIGHT x2 now green; the 4 left are 0.6.x deltas of later waves (FILE claude-engine keys, WALK `context_window`, RATE x2 F06-1/F06-2) and fail identically on main | completed |
| harness-panel | the same, `-Only harness-panel` | 42af7a9 | 1 | 60/2 (main 52/10): all 8 LIGHT green; SPEC x2 identical on main (F07-1/F11-6) | completed |
| harness-lock2 | the same | 42af7a9 | 0 | 11/0 | completed |
| harness-fixes E27 | the same, `-Only harness-fixes` (E27 section) | 42af7a9 | 1 | 9/2 (main 4/7); E28 x2 = unknown-tree recovery after an unconfirmed kill, wave 3 | completed |
| harness-fixes28e ANCHOR | the same | 42af7a9 | 0 | 4/0 (main 0/4) | completed |

## Open findings

F02-5 `implemented` (the state corrected); F02-1..F02-4, F02-6..F02-8 `proposed` (assigned to waves 1b-4; F02-6
is this wave's D - please check it against the fixes28e ANCHOR fixtures).

## Questions

- **Q1.** A: a sample or a clock case the port gets wrong versus `Get-RetryAfter` (write-time zone, the consult
  clock, the two extras)? Should the extras stay?
- **Q2.** B: a Codex app process the exclusion misses, or a reviewer it would hide?
- **Q3.** C and D: faithful to the plugin's texts and order (the plan lines, the anchor)? F02-6 closable?
- **Q4.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 600 words.
