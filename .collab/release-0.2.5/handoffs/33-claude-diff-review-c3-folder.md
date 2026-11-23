# Handoff 33 - Claude: diff-review of 0.2.5 - C3's project folder is `.c3/`

Date: 2026-11-23. Range: `d37a397..70ed824` (base `d37a397`, head `70ed824`; one commit). The commit
also holds another piece of work (the `c3 maintain` / telemetry / CI debt); the files of THIS brief
are `crates/c3/src/state_dir.rs` (new), `crates/c3/src/{setup,finish,pack,cli}/*`,
`crates/c3/src/{mcp/mod,providers,work/mod}.rs`, the templates, `AGENTS.md`, `docs/DESIGN.md`, the
plugin skill files and the tests `finish.rs`, `init_doctor.rs`, `snapshot_partb.rs`,
`delegation_rule.rs`. Ignore the rest.

You run in a throwaway checkout of the head of this range; the change is the patch file named in
your prompt (`.collab/`, `.c3/` and `.eck/` left out). You may have no shell: read the patch and the
files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, lose or split a project's records, send something
private off the machine, or report a result that is not true - not style.

## Delta since the last review

First review of this code; no model has read it. C3 inherited its per-project folder `.eck/` from
another tool; it now keeps everything in `.c3/`. One piece:

- **Name owner** (`state_dir.rs`): the folder names (`.c3`, legacy `.eck`), the ignore-file names,
  `ensure`, the one-time move and `collab_dir` live in this module.
- **What lives in `.c3/`** (git-ignored as a whole): the manifests and `profiles.json`,
  `last-report.md` (was `lastsnapshot/AnswerToSA.md`), `scouts/`, `snapshots/`, and the cache that
  lived in `<collab>/.c3/` (`anchor`, `snapshot_seq`, `index/`, `explains/`), now one per
  repository.
- **Who creates it:** every command that writes into the folder (`init`, `finish`, `fail`,
  `manifest` and their MCP tools, `scout`, `fetch`, `snapshot`, `index build`, `explain`) creates
  it, with a `[STUB: ...]` file per missing manifest and a `.gitignore` line. The
  SessionStart hook, a read-only `c3 doctor` (a note, not a failure) and a C3 child (`C3_CHILD`)
  never do; in a child a writing command refuses when `.c3/` is missing.
- **The one-time move** (`setup/*`, `finish/*`, `cli/doctor.rs`): when `.c3` does not exist and
  `.eck` is a real directory, the first writing command (or `c3 doctor --fix`) renames it - one
  rename or nothing, never a link, never over an existing path, never while a detached
  consultation or work run is live under `.eck/`. A failed rename is not retried another way: the
  command says why and uses `.eck/` for that run. Then `anchor`, `update_seq` and
  `telemetry_queue.json` go to `.c3/legacy/`, the old report to `last-report.md`, and a
  `<collab>/.c3/` cache moves entry by entry (an existing destination wins). Both folders present:
  nothing is merged, `c3 doctor` warns. `C3_STATE_MIGRATE=off` (or `"state_migrate": false`) keeps
  C3's hands off `.eck/`. Until a move, `c3 doctor` and the manifest digest read `.eck/`.
- **A collab directory inside the folder** (`--collab-dir .eck/collab`): once `.c3/` exists a
  vanished `.eck/...` path is read as the same path under `.c3/`; a `.c3/...` path on a repository
  that still has `.eck/` moves the folder first, or falls back to the legacy path when the rename
  cannot be made.
- **No snapshot by default** (`finish/*`, `cli/finish.rs`, `mcp/mod.rs`): `c3 finish` / `c3 fail`
  and `c3_finish_task` / `c3_fail_task` write the report, the journal entry and (finish) the commit;
  `--snapshot` asks for the hand-off snapshot; `--no-snapshot` is accepted and does nothing; plain
  `c3 snapshot` output goes to `.c3/snapshots/`.
- **Old names still read** (`pack/*`, `cli/snapshot.rs`): `--state` (hidden alias `--eck`);
  `.c3ignore`, else `.eckignore`; the stub detector takes `[STUB]` and `[STUB:`. Reviewer packs,
  snapshots, `c3 fetch` and `--diff` patches leave out both `.c3/` and `.eck/` (case-insensitively
  on Windows and macOS).
- **Rules files** (`setup/rules.rs`, `setup/doctor.rs`): `c3 doctor` reports a C3-written rules
  file with an old template sentence naming `.eck/`; `--fix` replaces exactly that line through the
  link-safe temp-file-and-rename path; user-written text is left alone.

## CURRENT invariants claimed

- A project's folder is moved by one rename or not at all - never half; a failed rename leaves
  `.eck/` as it was.
- Nothing is merged or overwritten when both folders exist; an existing destination entry wins.
- A link named `.eck` (symlink, junction) is never moved and its target is never touched.
- The hook, a read-only command and a C3 child never create or move the folder.
- No move starts while a detached consultation or work run is live under `.eck/`.
- A consultation's records are found under either spelling of the collab path after the move.
- With the switch off nothing under `.eck/` is touched; C3 creates a fresh `.c3/`.
- Packs, snapshots, `c3 fetch` and patches leave out both `.c3/` and `.eck/`.
- `c3 finish` / `c3 fail` write no snapshot unless asked.
- `c3 doctor --fix` rewrites only the old template lines of a C3-written rules file.

## Evidence

- fmt and clippy clean; c3-core 252 and c3 library 852 tests; c3-cli `finish` 18, `init_doctor` 19,
  `snapshot_partb` 7, `delegation_rule` 7, `mcp_integration` 2; every other c3 binary green.
  `branches`, `ci_check`, `update`, `claude_engine`, `kimi_engine` and `work` also ran; CI runs all.
- CI for `70ed824` was still running when this brief was handed over; the commit before it is
  green on Linux and Windows.
- NOT exercised live: no real project folder has been moved yet (this repository's own is moved
  after the release); no run was live under a folder during a move.

## Questions

- **Q1. Half-done or repeated move.** Name any path where the move is half done or done twice: two
  `c3` processes at once, a crash between the rename and the leftovers step (`legacy/`,
  `last-report.md`, the cache entries, the `.gitignore` line), a second writing command during the
  first. Is the state after each crash point repaired by the next run or left split?
- **Q2. Lost or split records.** Name any path where records land in `.eck/` after `.c3/` exists or
  vice versa: an old binary and a new one used in turn, a detached run started before the move,
  the cache moved entry by entry (a half-moved `index/`). Can a pass read the wrong `anchor`?
- **Q3. Links and path safety.** What happens when `.eck` or `.c3` is a symlink, a junction or sits
  on another volume, or when `legacy/` or `last-report.md` already exists? Can any step write
  through a link or over an existing file?
- **Q4. Who may create or move.** Can the hook, a C3 child, a read-only command (`doctor`, `pack`,
  `explain`, `index` queries) or an MCP tool create or move the folder when it should not? Is
  `C3_STATE_MIGRATE=off` honoured by every writer, the MCP tools included?
- **Q5. Collab path mapping.** Name any path where the `.eck` to `.c3` rewrite of a collab
  directory picks the wrong place: absolute paths, `..` components, case (Windows vs Linux), a
  custom collab directory elsewhere, a path that exists in both spellings.
- **Q6. Snapshot default.** Does anything still expect a snapshot after `c3 finish` or
  `lastsnapshot/AnswerToSA.md` (a skill, a hook, the plugin, `c3 doctor`, the MCP result text)?
  Does any exclusion miss one spelling of the folder?
- **Q7. Claims.** Does a CHANGELOG, README or help sentence claim more than the code does?
- **Q8.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
