//! 观测面**包内**读一份记录的四层只许往下引：读行（`lines`）→ 认行（`recognize`）→ 扫描图与骨架（`record_scan` · `skeleton`）
//! → 查询（`query`）→ 跟读（`watcher`）。包与包之间的方向由 Cargo 的依赖图管（引了没列进依赖的就编不过）；
//! 同一个包里的模块之间编译器不管，这一条扫生产段补上。

use guard_core::production_code;

/// `(谁, 不许引的那几个兄弟, 为什么是这个方向)`；兄弟写作 `a · b · c`。
const DOWNWARD_ONLY: &[(&str, &str, &str)] = &[
    (
        "lines",
        "recognize · record_scan · skeleton · query · watcher",
        "读行在最下：一行行读 · 算不算一行 · 一行怎么解析",
    ),
    (
        "recognize",
        "record_scan · skeleton · query · watcher",
        "认行只认给它的那一行，不碰文件、不问扫描图",
    ),
    (
        "record_scan",
        "query · watcher",
        "扫描图一份记录扫一趟，查询与跟读都来要它",
    ),
    ("query", "watcher", "查询只读扫描图；跟读起步也问扫描图"),
];

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// 观测面里的一层（`layer.rs` 或 `layer/` 底下）引了哪几个不许引的兄弟：`(处数, 文件, 兄弟)`。
/// 认 `crate::observe::x` · `super::x`（包根那一级的文件）· `super::super::x`（下一级目录里的文件）。
fn upward_refs(layer: &str, not: &str) -> Vec<(usize, String, String)> {
    let root = guard_core::addr::backend_src_root().join("observe");
    let targets: Vec<&str> = not.split(" · ").collect();
    let files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .filter(|(p, _)| {
            let rel = p.strip_prefix(&root).unwrap_or(p);
            rel == std::path::Path::new(&format!("{layer}.rs")) || rel.starts_with(layer)
        })
        .collect();
    assert!(
        !files.is_empty(),
        "`observe::{layer}` 一份文件都没有 —— 这一行是空转"
    );
    let mut out = Vec::new();
    for (p, raw) in files {
        let nested = p.parent() != Some(root.as_path());
        let up = if nested { "super::super::" } else { "super::" };
        let src = production_code(&raw);
        for t in &targets {
            let n = [format!("crate::observe::{t}"), format!("{up}{t}")]
                .iter()
                .map(|needle| {
                    src.match_indices(needle.as_str())
                        .filter(|(i, _)| {
                            let after = src[i + needle.len()..].chars().next();
                            let before = src[..*i].chars().next_back();
                            !after.is_some_and(is_ident_char)
                                && !before.is_some_and(|c| is_ident_char(c) || c == ':')
                        })
                        .count()
                })
                .sum::<usize>();
            if n > 0 {
                let rel = p.strip_prefix(&root).unwrap_or(&p).display().to_string();
                out.push((n, rel, t.to_string()));
            }
        }
    }
    out
}

/// ★ 读一份记录的四层只许往下引（生产段零处向上）。
#[test]
fn the_record_reading_layers_only_reference_downward() {
    let mut bad = Vec::new();
    for (who, not, why) in DOWNWARD_ONLY {
        for (n, file, to) in upward_refs(who, not) {
            bad.push(format!(
                "observe::{who} → observe::{to}：{n} 处（{file}；{who} 在 {to} 之下：{why}）"
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "观测面读记录的几层方向不对：\n  {}",
        bad.join("\n  ")
    );
}
