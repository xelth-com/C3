All green. Final report:

## Report

### Files changed
- `crates/c3/src/consult/prompt.rs` — 5 new tests in `mod tests` (cases 1, 2, 3, 5, 6)
- `crates/c3-core/src/lineage.rs` — new `#[cfg(test)] mod tests` at end of file with 1 test (case 4); the file had no test module, written in the crate's style (`use super::*`, `#[cfg(test)] mod tests`, doc-comment header absent like most siblings)

No non-test code touched. (`cargo fmt` reformatted one line of my own new test only.)

### Case list
1. Non-blank `brief_text` with `brief_ref` set: prompt contains `<brief path=".collab/t/handoffs/01.md">`, the text, `</brief>`; block AFTER "Read the brief at", BEFORE "Review purpose:".
2. Empty and whitespace-only `brief_text`: no `<brief` anywhere; the "Read the brief at" pointer line still present.
3. Text's trailing whitespace/newlines trimmed before `</brief>` (asserted via the exact block `<brief path="…">\r\n1. One.\r\n2. Two.\r\n</brief>` for text ending `"\r\n\r\n   \r\n"`).
4. `engine_spec("grok").inline_brief` is true, false for every other engine the function knows — iterated `ALL_ENGINE_NAMES` (codex, agy, muse, claude, grok, http).
5. (extra) `raw` mode still inlines the block after the pointer — raw drops the contract/schema scaffolding, not the brief section.
6. (extra) Non-blank `brief_text` with empty `brief_ref` inlines nothing — the guard is nested under the pointer (reachable only via `prompt.rs`'s contract, since `orchestrate.rs` gates the file read on `brief_path`).

### Verification (exact commands, run sequentially)
- `cargo test -j 2 -p c3 consult::prompt` → **13 passed, 0 failed** (8 pre-existing + 5 new)
- `cargo test -j 2 -p c3-core lineage` → **2 passed, 0 failed** in the lib (my `lineage::tests::inline_brief_is_grok_only` + pre-existing `engine::tests::cross_lineage_resume_is_refused`); 1 passed in an integration binary
- `cargo clippy -j 2 -p c3 -p c3-core --all-targets -- -D warnings` → clean (Finished, no warnings)
- `cargo fmt --check` → initially 1 diff in my new test; ran `cargo fmt`, re-check clean; re-ran the prompt tests (13 passed, 0 failed)

### Deviations
None. Case numbering in comments follows the list above; the brief's own items 1–4 map to my cases 1–4.

### Observations on the change itself (not fixed, per instructions)
- The change looks correct and minimal. Two notes for the coordinator:
  - `orchestrate.rs:3420` uses `std::fs::read_to_string(bp).unwrap_or_default()`: an unreadable brief file silently inlines nothing — the reviewer still gets the "Read the brief at" pointer, so it degrades to the old behavior rather than failing. Acceptable, but silent.
  - Only *trailing* whitespace is trimmed; leading blank lines of the brief travel into the prompt. Matches the stated design, just noting it.
- I did not test the `orchestrate.rs` gating (`brief_text` read only when `spec.inline_brief`) — the brief confined tests to the two files' test modules, and that function is a large monolith with no test seam.