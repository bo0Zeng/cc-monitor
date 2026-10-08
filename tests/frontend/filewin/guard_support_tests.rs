//! （住址反空真）—— 本包那三个住址各点名一样盘上真有的东西，指错地方当场红。
use super::*;

#[test]
fn every_address_points_at_something_we_can_name() {
    for (name, dir, probe) in [
        ("crate_root", crate_root(), "Cargo.toml"),
        ("crate_src_root", crate_src_root(), "lib.rs"),
        ("repo_root", repo_root(), "package.json"),
    ] {
        assert!(
            dir.join(probe).exists(),
            "{name}() = {dir:?} 下没有 {probe} —— 这个住址指错了地方"
        );
    }
}

/// ★ **文件窗口生产代码里没有会 panic 的那几个口子，锁除外。**
///
/// 发布档 `panic = "abort"`（壳 `Cargo.toml [profile.release]`）：哪条线程一 panic —— 界面帧、拉取、传输、查找、预览、
/// 与后端的连接 —— 整个窗口进程当场没了（用户见过的 0xc0000409）。担保不了的一律把错误交回界面；担保得了的写清担保，不用 panic 兜。
///
/// 人群：本包 `src/` 下每份 `.rs` 的生产段（`guard_core::production_code`：剥注释与测试段；`guard_support.rs` 整份是测试段）。
/// 数的口子：`.unwrap()` · `.expect(` · `panic!` · `unreachable!` · `todo!` · `unimplemented!` · `assert!` · `assert_eq!` · `assert_ne!`
/// （`debug_assert*` 不数：发布档里没有）。
/// 唯一例外：紧跟在 `.lock()` 后面的 `.unwrap()` —— 锁只在持有者 panic 时中毒，而发布档里 panic 就是进程退出，中毒轮不到发生。
///
/// 买不到：下标越界 · 字符串切在字中间 · 整数除零（这几样形状太多，静态数不准）—— 那几族逐处过过一遍，答案在交回里。
#[test]
fn production_code_has_no_panic_points_except_lock_unwraps() {
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
    fn ident(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }
    let mut files = 0usize;
    let mut lock_unwraps = 0usize;
    let mut hits: Vec<String> = Vec::new();
    for (path, text) in
        guard_core::scan_tree_excluding(&crate_src_root(), &["rs"], &["guard_support.rs"])
    {
        files += 1;
        let prod = guard_core::production_code(&text);
        let rel = path
            .strip_prefix(crate_src_root())
            .unwrap_or(&path)
            .display()
            .to_string();
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
                if *mark == ".unwrap()" {
                    let tail: String = before
                        .chars()
                        .rev()
                        .take(".lock()".len())
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    if tail == ".lock()" {
                        lock_unwraps += 1;
                        continue;
                    }
                }
                let line = prod[..at].matches('\n').count() + 1;
                hits.push(format!("{rel}（生产段第 {line} 行）：{mark}"));
            }
        }
    }
    assert!(files > 20, "只扫到 {files} 份 —— 扫描面坏了");
    // 反空真：锁上的 unwrap 一处都没认出来 ⇒ 认法坏了（今天几百处）。
    assert!(
        lock_unwraps > 100,
        "锁上的 `.unwrap()` 只认出 {lock_unwraps} 处 —— 认法坏了，上面那条零命中不算数"
    );
    assert!(
        hits.is_empty(),
        "文件窗口生产代码里有会 panic 的口子（发布档 panic = abort ⇒ 整个窗口进程没了）：\n{}\n\
         ⇒ 担保不了的改成把错误交回界面；担保得了的换成不会 panic 的写法并写清担保。",
        hits.join("\n")
    );
}
