use super::*;

/// ★ [`READ_ARGS_CODES`] 就是读入参那一处真会回的码（两向）：本文件（`control/cli_args.rs`）生产源码里交给错误出口的蛇形字面量。
/// `--resolve` 与仓外 aterm 的冻结契约经这张表把这些码算进它的码全集 —— 这里多回一个码不登记，那边的金样就说了假话。
#[test]
fn the_read_args_codes_are_the_ones_that_section_returns() {
    let src = guard_core::strip_comment_lines(&crate::guard_support::production_code(
        include_str!("../../../src/backend/control/cli_args.rs"),
    ));
    for f in [
        "fn source_of(",
        "fn decode_argv(",
        "fn read_stdin_within<",
        "fn read_args<",
        "fn read_input<",
    ] {
        assert!(
            src.contains(f),
            "读入参那一处没有 `{f}` —— 读的不是那个文件"
        );
    }
    let seg = src.as_str();
    let mut found: Vec<String> = Vec::new();
    for (i, _) in seg.match_indices('(') {
        let after = seg[i + 1..].trim_start();
        let Some(rest) = after.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else { continue };
        let lit = &rest[..end];
        if rest[end + 1..].trim_start().starts_with(',')
            && lit.contains('_')
            && lit.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        {
            found.push(lit.to_string());
        }
    }
    found.sort();
    found.dedup();
    let mut want: Vec<String> = READ_ARGS_CODES.iter().map(|c| c.to_string()).collect();
    want.sort();
    assert_eq!(
        found, want,
        "读入参那一处回的码 ≠ `READ_ARGS_CODES`（两向）"
    );
}
