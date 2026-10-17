//! 剥法没剥歪：后端 · monitor · 文件窗口整棵树剥得干净，后端根源码面剥完承重项还在。

use super::rule;

/// ★ 语义钉：后端根源码面（`lib.rs` ＋ `main.rs`）的生产段必须含这几样东西。
///
/// 字节数地板挡不住「单个文件被剥空 / 剥过头」：这条用**语义锚点**直接钉住那一类失效。
/// 锚点随重构变时（U3 拆层时 `mod watcher;` 挪进了 `observe/mod.rs`，红过一次），
/// 不是把红的那条删掉了事，而是问「今天还剩哪些东西是承重的」，换成那些。
#[test]
fn main_production_section_keeps_its_load_bearing_items() {
    rule(|repo| {
        let prod = format!(
            "{}\n{}",
            repo.file("src/backend/lib.rs").prod(),
            repo.file("src/backend/main.rs").prod()
        );
        for anchor in [
            format!("const BUILD{}", "_ID"),
            format!("const STREAM{}", "_FLAGS"),
            format!("const SUBCOMMAND{}", "_OPTIONS"),
            format!("mod obser{}", "ve;"),
            format!("mod contr{}", "ol;"),
        ] {
            assert!(
                prod.contains(&anchor),
                "后端根源码面的生产段里找不到 `{anchor}` —— 剥过头了，扫描面正在静默缩水"
            );
        }
    });
}

/// 后端每个源文件都剥得干净：取生产段那一下入口就当场自检（测试属性 · 剥法认不出的测试模块 · 测试区间括号配平）。
#[test]
fn every_backend_file_strips_clean() {
    rule(|repo| {
        for f in repo.crate_tree("src/backend", &["rs"]) {
            f.prod();
        }
    });
}

/// monitor 与文件窗口同一条。
#[test]
fn every_monitor_file_strips_clean() {
    rule(|repo| {
        for pkg in ["src/frontend/shell", "src/frontend/filewin"] {
            for f in repo.crate_tree(pkg, &["rs"]) {
                f.prod();
            }
        }
    });
}
