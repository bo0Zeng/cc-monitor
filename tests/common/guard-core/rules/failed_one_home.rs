//! 交出去的失败那一形全仓只一份定义：`cmd-wire` 的 `failed` 模块。生产 Rust 里别处再写一个 `struct Failed` ⇒ 又抄了一份。
//!
//! 认法：剥了注释与测试段的生产代码里，`struct` 后面紧跟着的名字恰是 `Failed`（不连着别的标识符字符）。
//! 名字不同的同形抄本（字段照抄、换个名）这里看不见。

use super::{equal, rule};

const HOME: &str = "src/common/cmd-wire/src/failed.rs";

/// 一段代码里 `struct Failed` 出现几次。
fn hits(code: &str) -> usize {
    let b = code.as_bytes();
    let needle = b"struct Failed";
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    (0..b.len().saturating_sub(needle.len() - 1))
        .filter(|&i| {
            b[i..].starts_with(needle)
                && (i == 0 || !ident(b[i - 1]))
                && b.get(i + needle.len()).is_none_or(|&c| !ident(c))
        })
        .count()
}

#[test]
fn failed_shape_lives_only_in_cmd_wire() {
    rule(|repo| {
        assert_eq!(
            hits("pub struct Failed { a: u8 } struct FailedDir; struct Failed<T>; mystruct Failed"),
            2,
            "认法坏了"
        );
        let found: Vec<(String, usize)> = repo
            .under(&["src"], &["rs"])
            .into_iter()
            .map(|f| (f.rel.clone(), hits(f.prod())))
            .filter(|(_, n)| *n > 0)
            .collect();
        equal("生产 Rust 里 `struct Failed` 的住处", found, [(HOME.to_string(), 1)]);
    });
}
