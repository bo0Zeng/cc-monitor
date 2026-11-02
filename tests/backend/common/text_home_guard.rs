//! **截断加省略号 · 取首个非空行 · 压空白，Rust 生产代码只住 `copy-core/src/text.rs`** —— 扫描型判据。
//!
//! | # | 判据 | 正控 |
//! |---|---|---|
//! | T1 | 手写压空白（`split_whitespace()` 收集后 `.join(`）零处 | 写一处 ⇒ 红 |
//! | T2 | 手写首个非空行（`.lines()` 后 `.find(` 判 `is_empty()`）零处 | 写一处 ⇒ 红 |
//! | T3 | 「接一个省略号」的写法（`push('…')` · 字面量里占位符紧跟「…」收尾，如 `"{}…"`）只在豁免名单那几份、数目恰如登记（两向：名单那份没了那一处也红） | 手写一处截断加省略号 ⇒ 红 |
//!
//! 人群：`src/backend` 与 `src/common` 的生产代码（去注释、去测试段）。文案句里的「…」（理由 · 提示）不是截断，T3 不认；
//! 盲区：省略号经变量拼进来、或另写成 `\u{2026}` 的，T3 看不见。

use std::collections::BTreeMap;

/// 三样的家（只有它许写）。
const HOME: &str = "common/copy-core/src/text.rs";

/// 有「接一个省略号」写法、但不是「截到几个字再接省略号」的那几份（份 · 处数 · 为什么）。
const ELLIPSIS_EXEMPT: &[(&str, usize, &str)] = &[
    (
        "backend/agents/beat.rs",
        1,
        "命令概要里省掉后半截（`cd …`），不按字数截",
    ),
    (
        "backend/agents/claudecode/drift.rs",
        1,
        "样例按线上摘录的字节上限截（`cmd_wire::record::excerpt_of`）",
    ),
    (
        "common/relay-route-core/src/lib.rs",
        1,
        "上游地址里的密钥那一段换成省略号（遮，不截）",
    ),
];

/// 去注释、去测试段的生产代码，`src/` 下的相对路径。
fn production_files() -> Vec<(String, String)> {
    let src = guard_core::addr::repo_src_root();
    [src.join("backend"), src.join("common")]
        .iter()
        .flat_map(|root| guard_core::scan_tree_excluding(root, &["rs"], &[]))
        .map(|(p, code)| {
            let rel = p
                .strip_prefix(&src)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, guard_core::production_code(&code))
        })
        .filter(|(rel, _)| rel != HOME && !rel.starts_with("common/guard-core/"))
        .collect()
}

fn squeezed(src: &str) -> String {
    src.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 每一处 `from` 之后、到下一个 `;` 之前，有没有 `then`（同一条表达式里）。
fn chained(code: &str, from: &str, then: &[&str]) -> usize {
    code.match_indices(from)
        .filter(|(i, _)| {
            let rest = &code[i + from.len()..];
            let stmt = &rest[..rest.find(';').unwrap_or(rest.len())];
            then.iter().all(|t| stmt.contains(t))
        })
        .count()
}

/// 手写压空白的开头：按空白切开、直接收集（中间 `take(n)` 取头几个词的不算，那是挑词）。
const COLLAPSE_HEAD: &str = "split_whitespace().collect";

/// 「接一个省略号」的写法处数（注释已剥、空白已去）：`push('…')`，或字面量里占位符紧跟「…」收尾（`"{}…"` · `"{cut}…"`）。
fn ellipsis_appends(squeezed: &str) -> usize {
    squeezed.matches("push('…')").count() + squeezed.matches("}…\"").count()
}

#[test]
fn t1_no_hand_written_whitespace_collapse() {
    let hits: Vec<String> = production_files()
        .iter()
        .filter(|(_, c)| chained(&squeezed(c), COLLAPSE_HEAD, &[".join("]) > 0)
        .map(|(rel, _)| rel.clone())
        .collect();
    assert!(
        hits.is_empty(),
        "手写压空白（应调 copy_core::collapse_ws / clip_flat）：{hits:?}"
    );
}

#[test]
fn t2_no_hand_written_first_line() {
    let hits: Vec<String> = production_files()
        .iter()
        .filter(|(_, c)| chained(&squeezed(c), ".lines()", &[".find(", "is_empty()"]) > 0)
        .map(|(rel, _)| rel.clone())
        .collect();
    assert!(
        hits.is_empty(),
        "手写首个非空行（应调 copy_core::first_line）：{hits:?}"
    );
}

#[test]
fn t3_ellipsis_only_where_registered() {
    let found: BTreeMap<String, usize> = production_files()
        .into_iter()
        .map(|(rel, c)| (rel, ellipsis_appends(&squeezed(&c))))
        .filter(|(_, n)| *n > 0)
        .collect();
    let want: BTreeMap<String, usize> = ELLIPSIS_EXEMPT
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), *n))
        .collect();
    assert_eq!(
        found, want,
        "「接一个省略号」的写法与豁免名单对不上：多出的是手写截断（应调 copy_core::clip / clip_flat），少了的那条从名单删掉"
    );
}

/// 正控：三条判法认得出手写的那几形（判法坏了会零命中地绿）。
#[test]
fn the_scans_recognise_the_hand_written_shapes() {
    let collapse = "let t = s.split_whitespace().collect::<Vec<_>>().join(\" \");";
    assert_eq!(chained(&squeezed(collapse), COLLAPSE_HEAD, &[".join("]), 1);
    let two = "Some(l.split_whitespace().take(2).collect::<Vec<_>>().join(\" \"))";
    assert_eq!(chained(&squeezed(two), COLLAPSE_HEAD, &[".join("]), 0);
    let first = "let l = s.lines().map(str::trim).find(|l| !l.is_empty())?;";
    assert_eq!(
        chained(&squeezed(first), ".lines()", &[".find(", "is_empty()"]),
        1
    );
    let first2 = "s.lines().find(|l| !l.trim().is_empty()).unwrap_or(\"\");";
    assert_eq!(
        chained(&squeezed(first2), ".lines()", &[".find(", "is_empty()"]),
        1
    );
    let words = "let f: Vec<&str> = line.split_whitespace().collect();";
    assert_eq!(chained(&squeezed(words), COLLAPSE_HEAD, &[".join("]), 0);
    assert_eq!(ellipsis_appends(&squeezed("format!(\"{}…\", &s[..i])")), 1);
    assert_eq!(ellipsis_appends(&squeezed("format!(\"{cut}…\")")), 1);
    assert_eq!(ellipsis_appends(&squeezed("out.push('…');")), 1);
    // 文案句里的省略号不是截断。
    assert_eq!(
        ellipsis_appends(&squeezed("reason = \"等那一下（…）再说\"")),
        0
    );
}
