use std::collections::BTreeSet;
use std::path::PathBuf;

/// 设计篇那一族住在**仓外**（`claudecode-frontend/调研/`，而仓根是 `cc-monitor/`）。
///
/// 🔴 `调研/` 是**给定的**（理由与代价住生产侧模块头注）⇒ 找不到就 panic，**不跳过**。
fn design_dir() -> PathBuf {
    let d = repo_root().join("../调研/设计");
    assert!(
        d.is_dir(),
        "找不到设计篇那一族（期望在 {}）—— `调研/` 是给定的，\
         缺席不是一种要伺候的常态，它是坏了。正确的处置是把 `调研/` 带上，\
         不是给本条开豁免（理由住 `design_doc_registry` 的头注）。",
        d.display()
    );
    d
}

fn index_text() -> String {
    let p = repo_root().join("../调研/README.md");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不动那份索引（{}）：{e}", p.display()))
}

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`。
    crate::guard_support::repo_root()
}

/// 「整篇作废」这个标记在索引里长什么样。**运行期拼**，免得本文件自己成为一处命中。
fn dead_mark() -> String {
    format!("🚫 {}", "整篇作废")
}

/// 🔴 比之前先剥掉 markdown 的强调符。
///
/// **它不是宽容，是判对了维度**：本条判的是「标了没标作废」，而 `**加粗**` 是排版。
/// 现打的活样本：同一份索引里 `50` 那一行写的是「🚫 整篇作废」、`61` 那一行写的是
/// 「🚫 **整篇作废**」—— 两行是同一个意思，而不剥星号的话第二行会被判成「没标」。
/// ⚠ 那一红**是真的**（索引里两处拼法不一致），但它红在**排版**上、不在**语义**上
/// ⇒ 按「判对维度」修判据，不是去改索引迁就判据。
fn strip_emphasis(s: &str) -> String {
    s.replace('*', "")
}

/// 索引里那张表：`(篇名, 那一行的状态格)`。
///
/// 抠法用共享原语 [`guard_core::md_table_rows`]（通用 markdown 表格抠法，
/// 它替我们丢掉分隔行与首尾那对 `|` 切出来的空格子 —— 两个都是踩过的坑）。
fn index_rows() -> Vec<(String, String)> {
    let t = index_text();
    let mut out = Vec::new();
    for cells in guard_core::md_table_rows(&t) {
        if cells.len() < 3 {
            continue;
        }
        // 第一格形如 `` `设计/60-xxx.md` `` —— 只收这一形，别的表不会误入。
        let f = cells[0].trim_matches('`');
        if let Some(name) = f.strip_prefix("设计/") {
            if name.ends_with(".md") {
                out.push((name.to_string(), cells[1].clone()));
            }
        }
    }
    out
}

fn docs_on_disk() -> BTreeSet<String> {
    // 🔴 走共享遍历原语，不裸 `read_dir` —— `scanning_guard_registry` 那条棘轮逐字要求的，
    //    理由也是它写的：「判据在自己的登记表/注释/常量里找到自己 ⇒ **恒绿**，
    //    audit-0805 实测五次，五次都不是被判据变红发现的」⇒ 修法是**让它写不出来**。
    //
    // ⚠ 排除名单**刻意是空的**，而那不是偷懒：本判据住 `tests/bridge/`，被扫的树是
    //    仓外的 `调研/设计/` ⇒ **结构上不可能在语料里找到自己**。空名单在这个原语里
    //    是合法的（它自己的头注逐字：「名单为空时退化成『不排除』是合法的：调用方就是要整棵树」），
    //    而**空串**才是它拒绝的那一形（空串命中一切 ⇒ 静默空集）。
    guard_core::scan_tree_excluding(&design_dir(), &["md"], &[])
        .into_iter()
        .map(|(p, _)| {
            p.file_name()
                .and_then(|s| s.to_str())
                .expect("文件名不是 UTF-8")
                .to_string()
        })
        .collect()
}

/// 那一篇**自己**声明了整篇作废吗 —— 只看抬头那一段（前 12 行），
/// 因为篇中提到「某一节作废」不等于整篇作废。
fn declares_itself_dead(name: &str) -> bool {
    let p = design_dir().join(name);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不动 {}：{e}", p.display()));
    let head: String = s.lines().take(12).collect::<Vec<_>>().join("\n");
    guard_core::contains_word(&head, "已作废") || head.contains(&dead_mark())
}

#[test]
fn the_extractor_really_sees_the_status_column() {
    let rows = index_rows();
    assert!(
        rows.len() >= 20,
        "只从索引里抠出 {} 行 —— **抽取器坏了**，下面那几条会拿两个几乎空的集合比出绿。\
         （索引那张表的形状变了？第一格不再是 `` `设计/NN-…md` `` 了？）",
        rows.len()
    );
    // 状态栏必须真的有内容，否则「标没标作废」这一维恒假。
    let with_status = rows.iter().filter(|(_, s)| !s.is_empty()).count();
    assert_eq!(
        with_status,
        rows.len(),
        "有 {} 行的状态格是空的 —— 那一维在这些行上**判不了**，不许当成「没标作废」",
        rows.len() - with_status
    );
    // 合成对照：识别器认得出一段**故意**标了作废的文本。
    let synthetic = format!("| `设计/00-x.md` | {} | 随便 |", dead_mark());
    let got = guard_core::md_table_rows(&synthetic);
    assert_eq!(got.len(), 1, "通用抠法认不出一行合成的表格行 —— 尺子没跑");
    assert!(
        strip_emphasis(&got[0][1]).contains(&dead_mark()),
        "抠出来的状态格里没有那个标记 —— 下面两条此刻在空转"
    );
}

#[test]
fn the_index_lists_exactly_the_design_docs_on_disk() {
    let listed: BTreeSet<String> = index_rows().into_iter().map(|(f, _)| f).collect();
    let disk = docs_on_disk();
    let missing: Vec<&String> = disk.difference(&listed).collect();
    let phantom: Vec<&String> = listed.difference(&disk).collect();
    assert!(
        missing.is_empty() && phantom.is_empty(),
        "索引与盘面对不上（两向）：\n  盘上有而索引没列：{missing:?}\n  索引列了而盘上没有：{phantom:?}\n\
         ⇒ 索引腐掉比正文腐掉更贵：正文腐了读者还能读出矛盾，**索引腐了他根本不会去读那一篇**。"
    );
}

#[test]
fn every_doc_marked_dead_in_the_index_really_declares_itself_dead() {
    let bad: Vec<String> = index_rows()
        .into_iter()
        .filter(|(_, s)| strip_emphasis(s).contains(&dead_mark()))
        .filter(|(f, _)| !declares_itself_dead(f))
        .map(|(f, _)| f)
        .collect();
    assert!(
        bad.is_empty(),
        "索引把这几篇标成整篇作废，而那几篇**自己的抬头没有那句声明**：{bad:?}\n\
         ⇒ 两条出路：① 索引标错了 ⇒ 改索引；② 那篇真作废了 ⇒ 在它抬头写下来。\
         **不许靠删掉索引那一行了事** —— 删掉的是线索。"
    );
}

#[test]
fn every_doc_that_declares_itself_dead_is_marked_dead_in_the_index() {
    let rows = index_rows();
    let bad: Vec<String> = docs_on_disk()
        .into_iter()
        .filter(|f| declares_itself_dead(f))
        .filter(|f| {
            !rows
                .iter()
                .any(|(n, s)| n == f && strip_emphasis(s).contains(&dead_mark()))
        })
        .collect();
    assert!(
        bad.is_empty(),
        "这几篇**自己的抬头就写着作废**，而索引没把它们标成整篇作废：{bad:?}\n\
         🔴 **这一条才是治那个病的那一半** —— 2026-09-23 现打的活样本：一篇死文档的正文\
         从第一行起就喊着「我作废了」，而索引把它标成「已定」，\
         于是任何按索引挑活的人都会去读它。"
    );
}
