//! The agent report (`AnswerToSA.md`) and the journal entry (`.eck/JOURNAL.md`), both as
//! pure functions over text so the exact format is tested without touching disk.

/// The `AnswerToSA.md` body for `c3 finish`: `# Agent Report\n\n<status>\n`.
pub fn answer_finish(status: &str) -> String {
    format!("# Agent Report\n\n{}\n", status.trim_end())
}

/// The `AnswerToSA.md` body for `c3 fail`: `# Agent Report (BLOCKED/FAILED)\n\n<status>\n`.
pub fn answer_fail(status: &str) -> String {
    format!("# Agent Report (BLOCKED/FAILED)\n\n{}\n", status.trim_end())
}

/// The first non-empty line of `s`, trimmed (used as a journal entry's title). Falls back
/// to `fallback` when `s` has no non-empty line.
pub fn first_line<'a>(s: &'a str, fallback: &'a str) -> &'a str {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(fallback)
}

/// Prepend a journal entry to `existing` (the current `.eck/JOURNAL.md`, or empty when the
/// file is missing). The entry — `## <timestamp> - <title>\n\n<body>\n` — is inserted right
/// after the `# JOURNAL` title so the newest entry is first. A missing title is created.
pub fn insert_journal_entry(existing: &str, timestamp: &str, title: &str, body: &str) -> String {
    let entry = format!("## {timestamp} - {title}\n\n{}\n", body.trim_end());

    if existing.trim().is_empty() {
        return format!("# JOURNAL\n\n{entry}");
    }

    // Insert before the first existing entry header (`## ` at a line start); everything
    // before it is the title block.
    match first_entry_header(existing) {
        Some(idx) => {
            let head = existing[..idx].trim_end_matches('\n');
            let rest = &existing[idx..];
            format!("{head}\n\n{entry}\n{rest}")
        }
        None => {
            // A title (and maybe prose) but no entries yet: the whole thing is the head.
            let head = existing.trim_end_matches('\n');
            format!("{head}\n\n{entry}")
        }
    }
}

/// The byte index of the first `## ` header at a line start, if any.
fn first_entry_header(s: &str) -> Option<usize> {
    if s.starts_with("## ") {
        return Some(0);
    }
    s.match_indices("\n## ").next().map(|(i, _)| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS: &str = "2026-10-30 14:05";

    #[test]
    fn answer_bodies_match_the_spec() {
        assert_eq!(answer_finish("done"), "# Agent Report\n\ndone\n");
        assert_eq!(
            answer_fail("blocked"),
            "# Agent Report (BLOCKED/FAILED)\n\nblocked\n"
        );
    }

    #[test]
    fn empty_file_gets_title_and_entry() {
        let out = insert_journal_entry("", TS, "did a thing", "the status");
        assert_eq!(
            out,
            "# JOURNAL\n\n## 2026-10-30 14:05 - did a thing\n\nthe status\n"
        );
    }

    #[test]
    fn title_only_inserts_entry_after_title() {
        let out = insert_journal_entry("# JOURNAL\n", TS, "t", "b");
        assert_eq!(out, "# JOURNAL\n\n## 2026-10-30 14:05 - t\n\nb\n");
    }

    #[test]
    fn existing_entries_push_down_newest_first() {
        let existing = "# JOURNAL\n\n## 2026-10-29 10:00 - old\n\nolder body\n";
        let out = insert_journal_entry(existing, TS, "new", "new body");
        assert_eq!(
            out,
            "# JOURNAL\n\n## 2026-10-30 14:05 - new\n\nnew body\n\n## 2026-10-29 10:00 - old\n\nolder body\n"
        );
        // The new entry's header comes before the old one.
        let new_pos = out.find("- new").unwrap();
        let old_pos = out.find("- old").unwrap();
        assert!(new_pos < old_pos);
    }

    #[test]
    fn first_line_takes_the_first_non_empty_line() {
        assert_eq!(first_line("\n\nhello\nworld\n", "x"), "hello");
        assert_eq!(first_line("   \n", "fallback"), "fallback");
    }
}
