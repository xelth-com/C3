You are a coding worker inside this repository. Do only what the brief asks, touch only the files it names or clearly implies, never run git, never leave the repository, and never commit. Finish with a final report: what you changed (file list), the exact test commands you ran and their result counts, any deviation from the brief and why, and what you could not do.

# Worker brief 01 - tests for the grok inline brief (you are the TEST writer, not the author)

## The change you are testing (already in the working tree, written by the coordinator)

Problem: a grok reviewer (`--json-schema` turn) sometimes answers in ONE model call without
calling `read_file`, so it never sees the brief and invents one from the question numbers.
Fix: the brief's TEXT is inlined into the prompt for grok only.

- `crates/c3-core/src/lineage.rs` - new `EngineSpec` field `inline_brief: bool`; `true` only in
  the `"grok"` spec, `false` for codex, agy, muse, claude, http.
- `crates/c3/src/consult/prompt.rs` - new `PromptInputs` field `brief_text`; in `assemble`, right
  after the "Read the brief at `<ref>`" line, when `brief_text` is not blank, one more part:
  "The brief's full text, ..." followed by `<brief path="<ref>">`, the text (trailing whitespace
  trimmed), `</brief>`.
- `crates/c3/src/consult/orchestrate.rs` (search `inline_brief`) - reads the brief file into
  `brief_text` only when `spec.inline_brief`.

Read those three spots first. Do NOT change any non-test code. If you believe the change itself
is wrong, say so in your report instead of fixing it.

## What to do

Before writing tests, list in your report the cases the change must satisfy (a numbered list).
Then write one test per case and name the case number in a comment above each test.

At least:
1. `prompt.rs` `mod tests`: with `brief_text` set, the prompt contains `<brief path=".collab/t/handoffs/01.md">`,
   the text, and `</brief>`, and the inlined block comes AFTER "Read the brief at" and BEFORE
   "Review purpose:".
2. `prompt.rs`: empty and whitespace-only `brief_text` add nothing (no `<brief` in the prompt).
3. `prompt.rs`: trailing whitespace/newlines of the text are trimmed before `</brief>`.
4. `lineage.rs` tests (find its `mod tests`, or add one in the style of the file): `inline_brief`
   is true for `engine_spec("grok")` and false for every other engine name the function knows.
Add anything else you find missing (e.g. `raw` mode, a brief with no `brief_ref`), within these
two files' test modules.

Follow the style of the existing tests in each module (`base()` helper in `prompt.rs`).

## Verify

- `cargo test -j 2 -p c3 consult::prompt` and `cargo test -j 2 -p c3-core lineage` - all green.
- `cargo clippy -j 2 -p c3 -p c3-core --all-targets -- -D warnings` - clean.
- `cargo fmt --check` - clean (run `cargo fmt` if not).
Never run the full workspace test suite; never run two cargo commands at once.

## Report

Files changed, the case list, the exact commands with their pass counts, anything you think is
wrong with the change.
