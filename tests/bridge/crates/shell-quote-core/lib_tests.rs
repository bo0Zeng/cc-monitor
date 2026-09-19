use super::*;

#[test]
fn posix_quote_breaks_single_quotes_the_posix_way() {
    assert_eq!(posix_quote("/p"), "'/p'");
    assert_eq!(posix_quote("a'b"), "'a'\\''b'");
    assert_eq!(posix_quote(""), "''");
}
