//! 剥法没剥歪：后端整棵树剥得干净、根源码面剥完承重项还在；两处共用的那段散文逐字相同。

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

// ⟦KR115D3 共用段·起⟧
/// 这条同时是「列 0 收尾判据够不够用」的持续验证 —— **而它自己已经守不住那一形了**。
///
/// ⚠ 上一版这里逐字写着「哪天有人在测试模块里写了一段列 0 含右
/// 大括号的原始字符串，**这里会红**」。那句话**两天里假了两次**：
/// ① `K-R110`（09-13）现打它**当时就没红** —— 那 31 行漏进生产段，而守门人看不见；
/// ② `K-R110` 之后它**永远不会再因此红** —— 那一形已经被
///    `guard_core::assert_test_module_ranges_are_brace_balanced` 正确处理掉了
///    （匹配单位从「行」换成「计数」，区间在原始字符串里收尾会被它当场逮住）。
/// ⇒ **今天真正守这一形的是那条判据**；本条经剥法自检（`assert_tree_strips_clean` · 规则入口的 `File::prod`）调到它，
///    本条自己守的只剩「这棵树剥得干净」。
///
/// 🔴 **这一段在两处各住一份，逐字必须相同**
/// （`tests/common/guard-core/rules/strip_clean.rs` ＋ `tests/frontend/shell/structural_scan_tests.rs`）——
/// `K-R110` 交回时点名过这个形状：**两个住址、同一句话**，改一处漏一处，
/// 下一次还是一处真一处假。钉着它的是
/// `strip_clean.rs::the_two_strip_clean_notes_stay_one_sentence`，**只改一处当场红**。
// ⟦KR115D3 共用段·止⟧
#[test]
fn every_backend_file_strips_clean() {
    rule(|repo| {
        for f in repo.crate_tree("src/backend", &["rs"]) {
            f.prod();
        }
    });
}

/// 🔴 `K-R115` `KR115D3`：**「两个住址、同一句话」不许再各说各话。**
///
/// 上面那条与 monitor 那一侧的同名判据（`tests/frontend/shell/structural_scan_tests.rs`）头上挂着**同一段散文**；
/// 本条把那一段钉成**逐字相同**（只削行首空白：缩进是位置，不是话），只改一处当场红。
///
/// 买不到：两处一起改成同一句假话照样绿（那要读语义）；第三处再抄一遍看不见。
#[test]
fn the_two_strip_clean_notes_stay_one_sentence() {
    rule(|repo| {
        // 标记运行时拼：写成字面量的话，这两行自己就会命中标记，「恰好 1 次」当场恒假。
        let beg = format!("// ⟦KR115D3 共{}", "用段·起⟧");
        let end = format!("// ⟦KR115D3 共{}", "用段·止⟧");
        let pointer = format!("assert_test_module_ranges_are_brace{}", "_balanced");
        let mut blocks: Vec<(&str, String)> = Vec::new();
        for rel in [
            "tests/frontend/shell/structural_scan_tests.rs",
            "tests/common/guard-core/rules/strip_clean.rs",
        ] {
            let src = repo.file(rel).text();
            for (mark, which) in [(&beg, "起"), (&end, "止")] {
                let n = src.matches(mark.as_str()).count();
                assert_eq!(
                    n, 1,
                    "{rel} 里共用段的「{which}」标记出现 {n} 次，应当恰好 1 次"
                );
            }
            let i = src.find(beg.as_str()).unwrap() + beg.len();
            let j = src.find(end.as_str()).unwrap();
            assert!(i < j, "{rel} 里共用段的两个标记次序反了");
            let block: String = src[i..j]
                .trim_end()
                .lines()
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                block.lines().count() >= 10 && block.contains(pointer.as_str()),
                "{rel} 抽到的共用段太短或没点名今天真正守这一形的那条判据"
            );
            blocks.push((rel, block));
        }
        assert_eq!(
            blocks[0].1, blocks[1].1,
            "{} 与 {} 的共用段逐字对不上 ⇒ 有人只改了一处。两处一起改。",
            blocks[0].0, blocks[1].0
        );
    });
}
