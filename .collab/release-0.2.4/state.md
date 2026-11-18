# release-0.2.4 - state

- 2026-11-15: diff-review panel of the 0.2.4 wave (c3 maintain, grok prompt-only transport and
  fresh-conversation re-ask, z.ai / Muse quota sources, build commit in `--version`).
  - Brief: `.collab/release-0.2.4/handoffs/01-claude-diff-review.md`; the change as a patch:
    `.collab/release-0.2.4/handoffs/01-claude-diff-review.patch` (base 035d386 + working tree).
  - Panel: detach id `76fe4406` - ZAI-claude :: glm-5.3 (handoff 02), meta :: muse-wonder-1.0 (03),
    grok :: grok-4.7 (04); seated through a scratch roster of exactly these three, because the
    standing partner (openai :: gpt-6-astra) sits at 97 % of its weekly window until Nov 18 and the
    native kimi engine reviews a HEAD worktree that does not hold the uncommitted change.
  - Come back: `c3 consult --task release-0.2.4 --status --id 76fe4406`.
  - Run with the debug build of the wave itself (`target\debug\c3.exe`), so the grok member already
    takes the schema in the prompt.
- 2026-11-15, result: panel wall 15 min, all three usable. GLM-5.3 ACCEPT (4 minor, 3 note); Muse Wonder HOLD
  (1 blocker, 10 major, 1 minor); Grok HOLD (5 major, 5 minor, 1 note) - Grok made 84 tool calls on its first turn,
  no re-ask (its first run on the prompt-only schema transport). 30 findings, none rejected: 24 `verified` by a named
  test in the green full run (c3-core 281, c3 lib 634, c3-cli 188 in 19 binaries), 6 `implemented` without a
  dedicated test (F03-9, F02-1, F04-7, F02-4, F04-8, F03-12). All three consultations rated useful.
  - One defect came from a fix itself: the direct `CreateProcessW` spawn asked for by F04-8 inherited the hook's
    output pipes, so the hook "returned" only when the pass ended (30 s, seen live). Fixed (the hook's own standard
    handles are made non-inheritable first); regression test `the_hook_returns_without_waiting_for_the_spawned_pass`
    fails without the fix and passes with it; live: hook 2.1 s with a due pass, the pass ran 20 s in the background,
    the next hook printed the report once.
- 2026-11-15, second wave on top of d7ce658 - reviewer-protection routing (operator: "астра постоянно на передке
  часто убивает её в ноль"; Muse Wonder only on his explicit request). Brief
  `handoffs/05-claude-routing-review.md`, patch `05-claude-routing-review.patch`. Diff-review panel detach id
  `01480f5d`: meta :: muse-spark-1.3-contributor and grok :: grok-4.7 (the cheap pair, by the operator's rule; no
  Wonder, no Astra), run from a copy of the wave's debug build. Come back:
  `c3 consult --task release-0.2.4 --status --id 01480f5d`.
  Result: both HOLD (Grok 2 major / 2 minor, 14 min; Muse contributor 2 major / 1 minor, 4 min), 7 findings
  F06-1..4, F07-1..3, all held and fixed (`implemented`, each with a named test except F07-3); both rated useful.
  Full run after the fixes: c3-core 282, c3 lib 644, c3-cli 196 in 20 binaries, 0 failed.
- Acceptance by the standing partner: brief READY at `handoffs/08-claude-acceptance.md`, NOT run. On 2026-11-15
  the ChatGPT account was reset (weekly 25 % until Nov 22) but another session had its 5-hour window at 76 % until
  21:08, so a full acceptance would have hit the budget stop; and spending the standing reviewer is the operator's
  call the day he complained about it being drained. To run it:
  `c3 consult --task release-0.2.4 --purpose acceptance --mode new --brief .collab/release-0.2.4/handoffs/08-claude-acceptance.md --reply-name acceptance --detach`
  (no `--provider`: acceptance goes to the standing partner). Then `git tag v0.2.4` at the accepted commit.
- 2026-11-15 evening, on the operator's word ("запускай астру на приемку и пуш все что надо должна остаться одна
  мейн ветка"): the CI wave before the acceptance. GitHub CI had been red at its Format step since 2026-11-08
  (run 42525901306), so clippy and both Linux jobs had not run on any commit after 5018ac4. Done: `cargo fmt --all`
  (24 files), 9 clippy fixes without behaviour change (the roster engine-name regex built once before the loop,
  `budget/mod.rs`, `cli/{budget,router}.rs`, `engines/{grok,kimi}.rs`, `maintain/version.rs`, two tests), the Linux
  cfg gates of everything added since 5018ac4 read (no gap found; not compiled locally). Full run on the final tree:
  c3-core 282, c3 lib 644, c3-cli 196 in 20 binaries, 0 failed; fmt check and clippy `-D warnings` clean.
  CHANGELOG gained the entries the three 2026-11-08 commits never got (budget-aware panel routing, the native kimi
  engine, read-insist re-asks, the roster loader skipping an unknown engine); README gained kimi, `c3 maintain` and
  the `ext.reserve_percent` / `ext.on_request` settings. Brief 08 now names the base as `5018ac4..HEAD` and says
  outright that the 2026-11-08 commits were never reviewed by any model (new Q0).
  Branches: all 33 local branches and the two remote ones (`cloud/linux-port`, `claude/sharp-cannon-ocamev`) were
  fully merged into main and are deleted; the leftover agent worktree is removed. Only `main` remains.
- 2026-11-15 21:09, acceptance pass 1 (brief 08, handoff 09, detach id `0487a32b`, openai :: gpt-6-astra, mode new):
  FAILED - stopped at 332 s by the quota budget, no answer written. Cause: the reviewer delegated to three
  sub-agents (`maintain_review`, `budget_protection`, `kimi_roster`) although the bridge passed
  `-c features.multi_agent=false`; on codex-cli 0.169.0 (updated by hand 2026-11-14) that flag no longer removes
  the multi-agent v2 tools. A second codex run of another session (xelixir, two sub-agents) ran at the same time.
  Cost: the 5-hour window 0 -> 93 % (until 02:09), the week 28 % -> 42 %. Not rated (a plan-limit stop).
  - Recovered from the rollouts (`~/.codex/sessions/2026/11/15/`, the main thread and the two sub-agents that
    reported; reasoning and inter-agent messages are encrypted there) as R1-R7: R1-R3 `c3 maintain` (the pending
    record cleared after a failed install or rollback; write errors of the record swallowed; the swap's protection
    gate read before the smoke), R4 `on_request` + held reserve spent by an exact `--require`, R5 one aggregate
    `observed_at` for a merged quota reading, R6 a future-dated candidate merged before validation (the open part
    of F06-4), R7 the kimi / roster questions the third sub-agent never answered. The reviewer's direction was HOLD.
  - All confirmed and fixed with a named test each (table in `handoffs/10-claude-acceptance-2.md`), except R7b
    (refuted: a skipped unknown-engine entry keeps its position). R7a was a real defect: kimi reads the brief by
    reference inside a HEAD worktree, so an uncommitted brief was not there; it is now staged into the worktree.
    Not adopted: an exit status for an unresolved update (`maintain --run` stays "exit 0 always").
  - The sub-agent leak: every codex turn now also passes `-c agents.enabled=false` (proven on a GLM model forced
    to multi-agent v2 through the ZAI codex provider - with the old flag the tools `spawn_agent`, `send_message`,
    `wait_agent` are offered, with the new one they are gone; five probe turns, the ChatGPT account untouched), and
    the budget watch stops any turn that spawns a sub-agent anyway (`StopKind::Subagent`, class `operator`, the
    partial kept). An item that turns sub-agents off by one key only still gets the other switch.
  - Full run on the merged tree (2026-11-15 23:33): c3-core 286, c3 lib 647 and 81 in the crate's own integration
    tests, c3-cli 203 in 20 binaries, 0 failed; `cargo fmt --all -- --check` and clippy `-D warnings` clean.
  - Second pass: brief READY at `handoffs/10-claude-acceptance-2.md` (a delta brief for a NEW thread; the 09
    thread replays 668 KB per tool call). Not before 02:09 on 2026-11-16 (the 5-hour window).
- 2026-11-16 02:15, acceptance pass 2 (brief 10, handoff 11, detach id `9a28f0c8`, openai :: gpt-6-astra, mode new,
  thread `01a1e1ec-c2d2-7de3-9bda-760504cedd03`, `--budget 50`), reviewed 08121e5 with CI run 43770037495 green:
  usable reply after 783 s, **HOLD** - 1 blocker, 3 major (F11-1..F11-4). `sub-agents 0`: the two switches held on
  the ChatGPT provider (the first run there since the fix). Cost: the 5-hour window 0 -> 25 % (until 07:16), the
  week 42 % -> 48 %; 36 calls, 2.18 M input tokens of which 2.05 M cached. Rated useful.
  - R2-R6 closed by the reviewer; R1 half-closed (F11-1); R7b refuted by the reviewer too; 12 of the 13 prior
    findings it re-read are `fixed`, F06-4 `still-open` in one part (see below).
  - F11-1 (blocker): `handle_pending` returns "resolved" after a failed rollback and when there is no rollback
    path, so the next pass clears the pending record of an engine that still fails its smoke.
  - F11-2: a kimi turn that speaks, then calls tools, then ends without an answer is published as a usable,
    verified reply (the pre-tool text is the reply); the salvage keeps that text too.
  - F11-3: R7a stages the brief only for the primary kimi turn; read-insist / continuation / repair turns get a
    fresh HEAD worktree without it.
  - F11-4: the sub-agent guard scans the whole rollout, so a resumed thread whose HISTORY holds a `spawn_agent`
    call would be stopped although the current turn spawned nothing.
  - All four read against the code by the coordinator and confirmed; fixes in work (one worker, in place).
  - F06-4, the open part: `openai_windows` returns the rollout reading before it looks at the recorded one, and
    the quota scan filters by provider, not by account. Not taken into this wave: the recorded quota is written
    from the same rollouts (it cannot be newer than them on one Codex home), and the machine has one ChatGPT
    account; it stays in `.eck/TECH_DEBT.md`.
  - A third pass needs the operator's word (two passes have cost 20 points of the week). Given on 2026-11-16
    ("да конечно давай если надо", for the pass and for the release build).
  - Fixes committed as 7c1e91f (2026-11-18; the full run is of 2026-11-16 on that tree: c3-core 287, c3 649 lib +
    81 integration, c3-cli 208 in 20 binaries, 0 failed; CI run 44247193938 green). F11-1..F11-3 `verified`.
    The first full run failed in three places: rustfmt of one test; the older guard fixture in
    `crates/c3/tests/pending_liveness.rs`, which wrote its call line undated (it dates the line now); and one
    15 s `claude --version` probe of a fake launcher while another session's build loaded the machine (passed
    on the second run, nothing changed).
- 2026-11-18 07:07, acceptance pass 3 (brief 12, handoff 13, detach id `833eec75`, openai :: gpt-6-astra, mode new,
  thread `01a1ed44-2711-70b2-8371-bdb99aa350df`, `--budget 50`), reviewed 7c1e91f: usable reply after 246 s,
  **ACCEPT** - 0 blocker, 0 major, 1 minor (F13-1). `sub-agents 0`. Cost: the 5-hour window 0 -> 11 %, the week
  78 % -> 80 % (other sessions had taken it from 48 % to 78 % in two days; the entry was `held (reserve)`, which
  a standing purpose passes by design - `--spend-reserve` was not used). 10 calls, 458 k input tokens of which
  394 k cached. Rated useful.
  - F11-1, F11-2, F11-3 closed by the reviewer. F11-4: the historical-call case is corrected; what is left is
    F13-1, which supersedes it.
  - F13-1 (minor, confirmed at `crates/c3/src/engines/subprocess.rs:680`): the guard's time bound is the budget
    watch's `turn_start` - taken after the spawn, the `on_running` callback and the stdin write, then set two
    seconds back - not the launch itself. A historical call inside those two seconds would count; a current
    call made before a slow `on_running` returned would not (the child-rollout check is the backstop, and a
    codex reviewer cannot act before its prompt arrives on stdin). Not fixed in 0.2.4: a code change after the
    ACCEPT would need another pass. Recorded in `.eck/TECH_DEBT.md` with the reviewer's RC1 as the test to write.
  - F06-4 stays open in the same part; the reviewer: neither it nor the uncovered scenarios block the tag.
  - Not rerun by the reviewer: the test totals, fmt, clippy, CI. Its first-run checklist (seven observable
    items for the first real update, rollback, kimi run, resumed codex run, roster swap and reserve hold) is at
    the end of `handoffs/13-codex-acceptance-3.md`; none of those ran live yet.
- Acceptance authority: the panel members above were stand-ins; the standing partner's ACCEPT is handoff 13.
  **0.2.4 is accepted.** The tag `v0.2.4` (annotated) sits on the commit that records this acceptance, as
  `v0.2.3` does; its code is that of the reviewed 7c1e91f. The CHANGELOG date is the day of the tag, 2026-11-18.
