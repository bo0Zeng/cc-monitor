//! ★ **文件窗口生产代码里没有会 panic 的那几个口子，锁除外。**
//!
//! 发布档 `panic = "abort"`（壳 `Cargo.toml [profile.release]`）：哪条线程一 panic —— 界面帧、拉取、传输、查找、预览、
//! 与后端的连接 —— 整个窗口进程当场没了（用户见过的 0xc0000409）。担保不了的一律把错误交回界面；担保得了的写清担保，不用 panic 兜。
//!
//! 人群：文件窗口包（`src/frontend/filewin`）源码人群里每份 `.rs` 的生产段。
//! 数的口子：`.unwrap()` · `.expect(` · `panic!` · `unreachable!` · `todo!` · `unimplemented!` · `assert!` · `assert_eq!` · `assert_ne!`
//! （`debug_assert*` 不数：发布档里没有）。
//! 唯一例外：紧跟在 `.lock()` 后面的 `.unwrap()` —— 锁只在持有者 panic 时中毒，而发布档里 panic 就是进程退出，中毒轮不到发生。
//!
//! 买不到：下标越界 · 字符串切在字中间 · 整数除零（这几样形状太多，静态数不准）。

use super::{none, rule};

const MARKS: &[&str] = &[
    ".unwrap()",
    ".expect(",
    "panic!(",
    "unreachable!(",
    "todo!(",
    "unimplemented!(",
    "assert!(",
    "assert_eq!(",
    "assert_ne!(",
];

/// 一段生产段里的口子：`(行号, 口子)`；锁上的 `.unwrap()` 不算。
fn points(prod: &str) -> Vec<(usize, &'static str)> {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = Vec::new();
    for mark in MARKS {
        for (at, _) in prod.match_indices(mark) {
            let before = prod[..at].trim_end();
            // 宏名前面不许还连着标识符（`debug_assert!(` 不算 `assert!(`）。
            if !mark.starts_with('.')
                && before.len() == prod[..at].len()
                && prod[..at].chars().next_back().is_some_and(ident)
            {
                continue;
            }
            if *mark == ".unwrap()" && before.ends_with(".lock()") {
                continue;
            }
            out.push((prod[..at].matches('\n').count() + 1, *mark));
        }
    }
    out
}

#[test]
fn production_code_has_no_panic_points_except_lock_unwraps() {
    rule(|repo| {
        let hits: Vec<String> = repo
            .crate_tree("src/frontend/filewin", &["rs"])
            .into_iter()
            .flat_map(|f| {
                points(f.prod())
                    .into_iter()
                    .map(move |(line, mark)| format!("{}（生产段第 {line} 行）：{mark}", f.rel))
            })
            .collect();
        let sample = "fn a() {\n    x.unwrap();\n    m.lock().unwrap();\n    debug_assert!(y);\n}";
        assert_eq!(points(sample), [(2, ".unwrap()")], "认法认错了合成样本");
        none(
            "文件窗口生产代码里有会 panic 的口子（发布档 panic = abort ⇒ 整个窗口进程没了；担保不了的改成把错误交回界面，担保得了的换成不会 panic 的写法并写清担保）",
            hits,
            points(sample).len(),
        );
    });
}
