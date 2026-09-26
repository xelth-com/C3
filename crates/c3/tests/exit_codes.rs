//! The `-Provider` exit-code mapping (0 available / 2 unavailable / 3 unknown).

use c3::providers::exit_code_for_verdict;

#[test]
fn exit_codes() {
    assert_eq!(exit_code_for_verdict("available"), 0);
    assert_eq!(
        exit_code_for_verdict("unavailable (missing: env CC_TEST_KEY not set)"),
        2
    );
    assert_eq!(
        exit_code_for_verdict("unavailable (table unusable: ...)"),
        2
    );
    assert_eq!(exit_code_for_verdict("unknown (config unreadable: ...)"), 3);
    assert_eq!(exit_code_for_verdict("unknown (sign-in not checked)"), 3);
}
