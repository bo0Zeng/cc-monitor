//! **失败那一句不许直接是下层原话**（C-W18 在文案表外的那一半）。
//!
//! 文案机检（`tests/copy/copy-checks.ts` 的 J8 · C-W18）只看文案表里的模板：模板里接了原话型占位（`{e}` · `{detail}` …）就红。
//! 它看不见**不经文案表**、把下层原话（子进程 stderr · 系统报错的 `to_string()`）直接当那一句交出去的写法 ——
//! `fail("start_failed", err.trim().to_string(), …)` 就是这样漏过去的（WIN5 回归 · 新建会话框里那句是 ccm 的 stderr）。
//! 本规则补这一格：后端生产段里「码的字面量紧跟着一个原话表达式」那一形逐个数，只许出现在下面登记的那几处（只许变少）。
//!
//! 判法（文本近似，如实写）：去掉注释与空白后找 `"<码>",<原话变量>(.trim())?.to_string()`，
//! 原话变量认 `e` · `err` · `stderr` · `stdout` · `out`（下层报错 / 子进程输出在全仓惯用的名字）。
//! 换个变量名就看不见 —— 它挡的是「顺手写一行」，不是证明。

use super::{equal, rule};

/// 原话变量的名字。
const RAW_NAMES: &[&str] = &["e", "err", "stderr", "stdout", "out"];

/// 今天还在的那几处（文件 → 处数），各带理由。改好一处就减一（两向相等：多了红、少了不减也红）。
const LEDGER: &[(&str, usize, &str)] = &[
    ("src/backend/control/session_batch.rs", 1, "当 ccm 起自己那一趟：找不到自身可执行文件"),
    ("src/backend/faces/read_face.rs", 3, "读面开文件 / 序列化：待收进带原话的读法"),
];

/// 一份生产段文本里那一形的处数。
fn raw_said_sites(prod: &str) -> usize {
    let flat: String = crate::strip_comment_lines(prod)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut n = 0;
    for name in RAW_NAMES {
        for tail in [".to_string()", ".trim().to_string()"] {
            let needle = format!(",{name}{tail}");
            let mut from = 0;
            while let Some(at) = flat[from..].find(&needle) {
                let pos = from + at;
                from = pos + needle.len();
                // 前一格是变量名的一部分（`some_e,`）就不算；紧挨着逗号之前得是 `"<码>"`。
                let before = &flat[..pos];
                let Some(code) = before.strip_suffix('"') else {
                    continue;
                };
                let Some(open) = code.rfind('"') else {
                    continue;
                };
                let lit = &code[open + 1..];
                if !lit.is_empty() && lit.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                    n += 1;
                }
            }
        }
    }
    n
}

#[test]
fn a_failure_sentence_is_never_the_raw_words_directly() {
    rule(|repo| {
        let found = repo
            .crate_tree("src/backend", &["rs"])
            .into_iter()
            .map(|f| (f.rel.clone(), raw_said_sites(f.prod())))
            .filter(|(_, n)| *n > 0);
        equal(
            "失败那一句直接用了下层原话的那几处（多出来的：那一句从文案表取、原话进复制详情 `Fail.raw` · `Said::with_raw`，别抄进登记；少了的：改好了，把登记那一行减掉）",
            found,
            LEDGER.iter().map(|(f, n, _)| (f.to_string(), *n)),
        );
    });
}

/// 正控：认得出那几形，认不出的不误报。
#[test]
fn the_shape_is_recognised() {
    assert_eq!(
        raw_said_sites("Err(fail(\"start_failed\", err.trim().to_string(), None))"),
        1
    );
    assert_eq!(
        raw_said_sites(".map_err(|e| (\"failed\",\n    e.to_string()))"),
        1
    );
    assert_eq!(raw_said_sites("(\"io_failed\", stderr.to_string())"), 1);
    assert_eq!(
        raw_said_sites("fail(\"x\", copy_text(\"k\", &[]), None)"),
        0
    );
    assert_eq!(raw_said_sites("(\"x\", some_e.to_string())"), 0);
    assert_eq!(raw_said_sites("// fail(\"x\", e.to_string())"), 0);
    assert_eq!(raw_said_sites("format!(\"{}\", e.to_string())"), 0);
}
