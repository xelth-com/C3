# Handoff 50 - Claude: diff-review of 0.2.5 - the bridge debt (wave D1)

Date: 2026-11-24. Range: `ac26f5a..2ffa02a` (one commit).

You run in a throwaway checkout of the head of this range; the change is the patch file named in
your prompt (`.collab/`, `.c3/` and `.eck/` left out). You may have no shell: read the patch and
the files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository or a consultation record, send something private off the machine, or report a result
that is not true (a failed reviewer shown as usable, a usable one shown as failed) - not style.

## Delta since the last review

First review of this commit. It closes 24 items of the reviewer bridge's tech debt; several are
fixes for findings of earlier reviews (ids in brackets). `CHANGELOG.md`, section `[Unreleased]`,
has one entry per item.

- **Findings snapshot.** A single run reads the open findings once at its start and is judged
  against that snapshot, like a panel member (`consult/orchestrate/mod.rs`).
- **Bound inputs.** A brief or bound artifact that changes during a run withholds the reply for
  every engine that reads the live tree - agy, muse, grok, claude, kimi; codex re-hashes them after
  its last turn (`consult/orchestrate/{mod,tree}.rs`).
- **Death record.** When the run's pending record and its status note name different event
  streams, the newer file wins [F24-2, F10-7, F11-11] (`consult/detach.rs`).
- **agy steps.** Parameters merge at every depth, conflicting values are both kept; for an
  allow-listed reader a value under a path / file / dir / uri key is data, never a command
  [F24-4, F32-3] (`engines/agy.rs`, `engines/tree_check.rs`).
- **The reviewer prompt** leaves paths under the collab directory out of the uncommitted-paths
  list and its counts (`engines/worktree.rs`).
- **`--diff` patch** (`consult/reviewed.rs`): only the patch this run wrote and its temp name are
  exempt from the collab comparison [F27-3]; the handoffs directory is opened and created through
  directory handles, a link above it refuses [F43-1]; an EMPTY claim file unchanged for 60 s is
  taken over [F43-2, F44-1]; a failed placement removes the name only while it is still its own
  empty claim [F43-3]; excluded paths go through a temporary index instead of the command line
  [F28-2]; a patch that is not valid UTF-8 is recorded and announced as altered [F28-3]; the
  untracked files a one-revision patch carries are printed and recorded, and
  `--diff-tracked-only` leaves them out [F18-4, F19-2, F20-6].
- **`--rev`.** One resolution of HEAD at the start decides whether prior findings are withheld
  and is the recorded `live_head`; an unresolvable HEAD withholds them [F29-3]. A brief missing
  from the live tree is accepted when it is committed at HEAD or at the reviewed revision, and the
  handoff says it was taken from the revision [F10-3, F11-4].
- **Router.** `c3 router explain` starts no process and uses no network; availability comes from
  recorded health only, an entry without a record is marked `[availability not checked]`; it
  says in which order it judged [F31-6, F30-1]. A missed upper tier is warned about when the
  required entries fill every seat [F30-2]. A replay treats a draw block with an invalid row as
  absent [F30-4] (`cli/router.rs`, `panel/plan.rs`, `router/replay.rs`).
- **`c3 providers`** counts a kept reply only while its file is still there [F11-12].
- **Tests only:** a `--rev` panel with two fake reviewers; a re-seat from the reserve after a
  staging refusal; the reply normaliser's timing test became a linearity test.

## CURRENT invariants claimed

- A reply is never recorded as usable when the brief or a bound artifact changed while a
  live-tree reviewer was reading.
- An agy step that carries a command anywhere in its merged parameters is classified as writing;
  a reader whose file name merely looks like a command is not.
- No `--diff` patch file is ever overwritten with other bytes, written through a link, or left
  half-written under its final name; an abandoned claim is taken over only when the file is empty
  and at least 60 s old.
- The patch a reviewer is told to read is the file whose sha256 the ledger records; a patch
  altered by redaction or by a lossy conversion says so.
- With `--diff-tracked-only` no untracked file's content is in the patch.
- Under `--rev`, prior findings are never judged against a revision other than the one they were
  recorded for.
- `c3 router explain` and `c3 router replay` never start a process and never change a file.

## Evidence

- The author's runs: c3 library tests 876, c3-cli `panel_light_required` 5, `kimi_engine` 21,
  `compat_wave2b` 21; fmt and clippy `-D warnings` clean. On the release branch the commit was
  merged on top of two others (two hand-merged conflicts); clippy clean there, CI for `2ffa02a`
  was running when this brief was handed over.
- NOT covered: an end-to-end run of the http engine under `--rev` (unit level only); the live
  tree half of the bound-input rule against a real agy / muse / grok CLI; a real file system
  without hard links for the patch placement (a test hook forces that path).

## Questions

- **Q1. Outcomes.** Name any path where a reviewer's reply is now withheld or failed although
  nothing it depends on changed (the bridge's own writes, a sibling panel member's patch, a
  timestamp-only change), or still recorded usable after a bound input changed.
- **Q2. agy classifier.** A writing step that the new merge or the "path keys are data" rule now
  lets through as a reader - nested keys, arrays, a command under a key that ends in `file` or
  `path`, a key the allow-listed reader does not have.
- **Q3. Patch placement.** Two or three runs at once, a crash at each step, a file system without
  hard links: a patch lost, replaced, left partial, or a claim that is taken over while its owner
  is alive and slow. Does the temporary index ever touch the real index or stay behind?
- **Q4. What leaves the machine.** With and without `--diff-tracked-only`: can untracked or
  ignored content reach the patch or the prompt; are the ten printed names safe to print; does a
  detached run or a panel member drop the flag?
- **Q5. `--rev` records.** A run whose HEAD moves while it runs, an unresolvable HEAD, a brief
  taken from the revision: is any record (handoff, ledger, a finding's source) wrong about what
  was read?
- **Q6. Router.** Does `explain` still start anything or reach the network on some path; can a
  replay now refuse or change a record it replayed before?
- **Q7.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
