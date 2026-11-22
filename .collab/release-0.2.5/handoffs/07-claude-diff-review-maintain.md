# Handoff 07 - Claude: diff-review of 0.2.5, part 3 of 5 - c3 maintain public catalogs

Date: 2026-11-22. Range: `23b1455..8e6fd3e` (base `23b1455`, head `8e6fd3e`; one commit).

You run in a throwaway checkout of the head commit of this range; the change itself is the patch
file named in your prompt (`.collab/` and `.eck/` left out). You may have no shell: read the patch
and the files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, send something private off the machine, or report a
result that is not true (a failed reviewer shown as usable, a usable one shown as failed) - not
style.

## Delta since the last review

A first panel reviewed `v0.2.4..ba43ff6` and returned 36 findings. This range holds (a) the fixes
for the findings on `c3 maintain` (F02-12 to F02-18, F04-2, F04-3, F04-8 for the fetch) and (b) one
tech-debt item (an unresolved pending update froze its engine). One piece:

- **P3. `c3 maintain` catalog and swap hardening** (`8e6fd3e`; `crates/c3/src/maintain/models.rs`,
  `crates/c3/src/maintain/web.rs`, `crates/c3/src/maintain/env.rs`,
  `crates/c3/src/maintain/smoke.rs`, `crates/c3/src/maintain/mod.rs`,
  `crates/c3/src/maintain/engine.rs`, tests in `crates/c3-cli/tests/maintain.rs`): public GETs
  with their own redirect policy and one deadline; a date gate that remembers refused pairs
  (`models.older`); endpoint lists keyed by endpoint, key variable and plan; the smoke and swap
  through a one-entry roster; a roster writer that edits the parsed value and verifies; `off`
  holding for `--models`; a pending update that asks the vendor for a newer release.

## CURRENT invariants claimed

- A public catalog or page GET is https and keyless, follows at most 3 redirects, each to the start
  URL's own host and port with no userinfo (re-checked at every hop), runs under ONE absolute 15 s
  deadline that also bounds the body read, and stops at 4 MiB + 1 bytes. A key-carrying endpoint
  request follows no redirect. The installer keeps only its `code.kimi.ai` -> `cdn.kimi.ai` pair.
- Every id passes the strict model-id shape before it is stored, compared, printed or written.
- A public candidate needs date evidence (created after the seated id when both are listed, else
  within 180 days); an undated entry or a page id is announced only; a date over a day in the
  future is refused. If ANY public source dates a candidate not after the seated id, it is refused
  whatever another source says; the pair is stored (`models.older`, at most 500) and stays refused.
  An id from the entry's own list meets the same refusals when a public source dates both ids.
- An endpoint's list is recorded as `endpoint:<scheme>://<host>:<port><path>|<key variable>|<plan>`
  and offers candidates only to entries with that key.
- Both smoke turns and the swap run against a one-entry roster (`CODEX_CONSULT_ROSTER`, written
  beside the scratch repository and removed with it) holding exactly the entry at the candidate's
  position with only `model` changed; the position must still be that provider and old model; a
  protected, on-request or inactive entry is refused even there. A codex custom-provider
  smoke runs with no roster; an explicit `C3_MAINTAIN_SMOKE_<ENGINE>` is resolved by label.
- The roster writer locates the string value of `reviewers[<position>].model` by parsing (a
  repeated key on the path is refused), checks it still decodes to the old model, replaces the
  whole literal, and verifies that the re-parsed roster equals the old one except that field.
- `C3_MAINTAIN_MODELS=off` makes `c3 maintain --models` fetch nothing and write nothing. `--models`
  leaves `next_due` and engine records alone; `--dry-run` runs no smoke, edits no roster and writes
  no state.
- An unresolved pending update tries a vendor release NEWER than the failing version, with the
  same cool-down (unless `--force`), in-use check, write-before-install record and post-update
  smoke, keeping the original `previous` as the rollback target.
- A successor may cross a major (`qwen3.8-max -> qwen4-max`; an id with no minor counts as `.0`);
  a protected or on-request entry is never smoked or edited.

## Evidence

- Local suite after `8e6fd3e`: fmt, clippy `-D warnings`, every test binary of c3-core, c3 and
  c3-cli (40 binaries, 1349 passed, 0 failed). CI green on Linux and Windows (run 45039380914).
- Tests include two same-label stub endpoints (the on-request one first: both smoke turns reach the
  candidate's endpoint, the other none) and a no-rollback pending update with agy.
- Known and left open: smoke consult children lose `CODEX_CONSULT_TEST_*`.
- NOT exercised live: the real catalog, a redirecting host, a `model_pages` entry, a roster swap,
  a vendor release; the first weekly pass with this code is due 2026-11-24. Nothing in this part
  was exercised against a real reviewer CLI.

## Questions

- **Q1. Fetch (security).** Name any way a catalog, page or redirect reaches a host, port, scheme
  or address other than the configured one (IP literal, localhost, a private network, `https` to
  `http`, host case, trailing dot, IDN), outlives the 15 s deadline (a slow body, a stalled DNS
  lookup), or exceeds 4 MiB / 5000 ids in memory. Does the request carry a key or identifying
  header?
- **Q2. Hostile body.** Can a catalog body, page or endpoint list inject anything into the roster,
  a command line, a file path, the maintain state, the report or the hook line (an id that passes
  the shape but is dangerous in a path, a huge field, odd Unicode or date forms)?
- **Q3. Date gate and family.** Any path that seats an older, unrelated or wrong-family model, or
  smokes without date evidence: clock skew, another date format, a re-stamping source, the 500-pair
  cap evicting a refusal so that an old model comes back, odd ids across a major.
- **Q4. Smoke binding and money.** Can a smoke or swap run on another entry's endpoint, plan or key
  than the one being changed, or spend on a protected or on-request entry? Is the one-entry roster
  always removed? Does `--dry-run` or `--models` touch state, `next_due`, engine records or the
  roster on ANY branch, error paths included?
- **Q5. Roster writer and races.** Can the writer corrupt the roster (BOM, CRLF, escapes, a
  duplicate key, a partial write, a missing backup), or overwrite a hand edit made between read
  and write? Two `c3 maintain` processes, or maintain beside a consult.
- **Q6. Pending update and claims.** Can the newer-release path install something unsmoked, lose
  the original `previous`, loop, or run inside a C3 child? Windows vs Unix differences. Does a
  CHANGELOG sentence here claim more than the code does?
- **Q7.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
