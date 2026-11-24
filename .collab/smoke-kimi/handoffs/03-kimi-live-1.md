# Handoff 03 - Kimi (kimi): live-1

Date: 2026-11-24 13:30 local. Author: Kimi (kimi) (model kimi-code/kimi-for-coding, effort tier in the model id), kimi-cli (version unknown).
Reviewer: kimi :: kimi-code/kimi-for-coding [kimi] (provider from -Provider, model from -Model; engine kimi (C:\Users\Dmytro\.kimi-code\bin\kimi.exe); provider fingerprint 282da20c8e15; harness kimi-cli (version unknown)).
Preflight: ok: signed in (usable reply 15 min ago).
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 11 of 17 for -Provider kimi (engine applied).
Effort: nothing sent (requested high, mapping model-tier, by caps-v1: engine kimi, the tier is part of the model id; not confirmed by the provider). Consultation id: cc4af5d6-13a1-45a7-b3d8-f6fb26ae5846.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: none). Argv: `kimi -p Your brief is the file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-44a4086cf683499088fcd7071703d002.txt. Read it completely first, then follow it; answer in the format it requires. --output-format stream-json -m kimi-code/kimi-for-coding` (a pointer to the prompt file, inline in argv: --prompt).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-2ed7550d992341d4af8ff492cc3abb1e` of revision 681af51bd38feb8054f87ddb52a2e98e142895f7 (--rev HEAD; removed after the run).
Parent thread: (none - new thread; no thread of lineage kimi :: kimi-code/kimi-for-coding in this task's ledger; other lineage(s): kimi :: kimi-code/k3.1-pro [kimi]). Result thread: `session_ead72d04-e414-46dc-94b5-844c548aa5e8` (source: events).
Brief: `.collab/smoke-kimi/handoffs/02-claude-live-kimi-0.2.5.md` (sha256 1345fb3df106). Reviewed: 681af51, base 681af51bd38feb8054f87ddb52a2e98e142895f7, tree sha256 c3e61eacc335, 0 changed files. Live HEAD at launch: 681af51bd38feb8054f87ddb52a2e98e142895f7.
Bridge outcome: usable reply. Wall time: 15.7 s. Tokens: not reported by kimi.
Engine turns: 1.
Timeout: 420 s (-TimeoutSec); continuation after a timeout kill: up to 420 s.
Verdict: ADVISE - Both questioned behaviors are confirmed directly in the table at revision 681af51; no defects found. Findings: none. Structured reply (prompt-only transport): `handoffs/03-kimi-live-1.reply.json`.
Raw event stream: `handoffs/03-kimi-live-1.events.jsonl`.
Verbatim reply follows.

---

**Q1.** Yes to both. U+202E is covered by `0x202A..=0x202E` at `crates/c3/src/unicode.rs:21` (RIGHT-TO-LEFT OVERRIDE is the inclusive upper bound). U+2066..U+2069 (the four bidi isolates LRI/RLI/FSI/PDI) are covered by `0x2066..=0x206F` at `crates/c3/src/unicode.rs:23`. The unit test also asserts both endpoints of each range are in (lines 70-71).

**Q2.** Example: U+E0001 (LANGUAGE TAG) — `| 0xE0001` at `crates/c3/src/unicode.rs:31`. Other non-BMP Cf entries include U+110BD (line 26), U+13430..=U+1343F (line 28), and U+1D173..=U+1D17A (line 30).

---

### Findings

_(none)_

### Prior findings

_(none)_

## Verdict: ADVISE

Both questioned behaviors are confirmed directly in the table at revision 681af51; no defects found.

### Blockers

_(none)_

### Unproven scenarios

- Test execution not observed (read-only consultation); correctness of the range assertions is inferred from reading the table and the test, not from a run.

### First-run checklist (observable)

_(none)_
