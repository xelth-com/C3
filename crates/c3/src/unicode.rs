//! (0.2.5 D5) The Unicode character classes C3 keeps out of what it shows: ONE table of the format
//! characters (general category Cf), used wherever a name is shown only when it is plain text - the
//! untracked files a `--diff` patch carries or leaves out (`consult::reviewed`), the branch name
//! of the red-CI line (`ci_check`) and (0.2.5 A4, F57-8) the tracked names the isolation prompt
//! lists (`engines::worktree`).

/// Whether `c` is a Unicode format character (general category Cf, Unicode 16.0): a bidi override
/// or isolate, a zero-width character, a soft hyphen, an invisible operator, a tag character... - it
/// can reorder or hide what a name shows.
pub(crate) fn is_format_char(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

#[cfg(test)]
mod tests {
    use super::is_format_char;

    // Every range of the Cf table is in at both ends, the neighbours around it are out, and so are
    // plain text, white space and control characters (each caller tests those on its own).
    #[test]
    fn the_format_characters_are_exactly_category_cf() {
        let ranges: [(u32, u32); 21] = [
            (0x00AD, 0x00AD),
            (0x0600, 0x0605),
            (0x061C, 0x061C),
            (0x06DD, 0x06DD),
            (0x070F, 0x070F),
            (0x0890, 0x0891),
            (0x08E2, 0x08E2),
            (0x180E, 0x180E),
            (0x200B, 0x200F),
            (0x202A, 0x202E),
            (0x2060, 0x2064),
            (0x2066, 0x206F),
            (0xFEFF, 0xFEFF),
            (0xFFF9, 0xFFFB),
            (0x110BD, 0x110BD),
            (0x110CD, 0x110CD),
            (0x13430, 0x1343F),
            (0x1BCA0, 0x1BCA3),
            (0x1D173, 0x1D17A),
            (0xE0001, 0xE0001),
            (0xE0020, 0xE007F),
        ];
        let ch = |u: u32| char::from_u32(u).unwrap();
        let mut count = 0;
        for (lo, hi) in ranges {
            assert!(is_format_char(ch(lo)), "U+{lo:04X}");
            assert!(is_format_char(ch(hi)), "U+{hi:04X}");
            assert!(!is_format_char(ch(lo - 1)), "U+{:04X}", lo - 1);
            assert!(!is_format_char(ch(hi + 1)), "U+{:04X}", hi + 1);
            count += hi - lo + 1;
        }
        // the whole table, counted over every scalar value: 170 characters, no others
        let all = (0..=0x10FFFFu32)
            .filter_map(char::from_u32)
            .filter(|c| is_format_char(*c))
            .count();
        assert_eq!(all as u32, count);
        assert_eq!(count, 170);
        for c in [
            'a', 'Z', '0', '-', '`', ' ', '\t', '\n', '\u{7f}', 'ä', '\u{2065}',
        ] {
            assert!(!is_format_char(c), "{c:?}");
        }
    }
}
