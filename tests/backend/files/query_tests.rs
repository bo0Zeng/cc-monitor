//! Everything 式搜索词：每种写法挑中的条目 == 预期集（两向相等），拒的那两形说得出是哪一处。
//!
//! 语料是手写的一摞路径（全合成），每条带类型；预期集逐条手写，不由被测代码派生。

use super::*;

/// 合成语料：(全路径, 类型)。
const CORPUS: &[(&str, u8)] = &[
    ("/h/u/Docs", KIND_DIR),
    ("/h/u/Docs/report.PDF", KIND_FILE),
    ("/h/u/Docs/report-2024.txt", KIND_FILE),
    ("/h/u/Docs/summer holiday.jpg", KIND_FILE),
    ("/h/u/src", KIND_DIR),
    ("/h/u/src/main.rs", KIND_FILE),
    ("/h/u/src/lib.rs", KIND_FILE),
    ("/h/u/src/report", KIND_DIR),
    ("/h/u/src/report/mod.rs", KIND_FILE),
    ("/h/u/.bashrc", KIND_FILE),
    ("/h/u/link-to-src", KIND_SYMLINK),
    ("/h/u/Ärger.md", KIND_FILE),
    ("/h/u/12:30 notes", KIND_FILE),
    ("/h/u/文档/报告.txt", KIND_FILE),
];

fn pick(q: &str) -> std::collections::BTreeSet<&'static str> {
    let m = parse(q).unwrap_or_else(|e| panic!("`{q}` 本该解析得了：{e:?}"));
    CORPUS
        .iter()
        .filter(|(p, k)| m.matches(p.as_bytes(), *k))
        .map(|(p, _)| *p)
        .collect()
}

fn set(v: &[&'static str]) -> std::collections::BTreeSet<&'static str> {
    v.iter().copied().collect()
}

#[test]
fn every_supported_form_selects_exactly_the_expected_entries() {
    let all: Vec<&'static str> = CORPUS.iter().map(|(p, _)| *p).collect();
    let cases: &[(&str, Vec<&'static str>)] = &[
        // 空白 ⇒ 全部。
        ("   ", all.clone()),
        // 只对名字、不分大小写：`docs` 不会因为父目录叫 Docs 就把里面的文件都挑出来。
        ("docs", vec!["/h/u/Docs"]),
        (
            "REPORT",
            vec![
                "/h/u/Docs/report.PDF",
                "/h/u/Docs/report-2024.txt",
                "/h/u/src/report",
            ],
        ),
        // 空格 ＝ 且。
        ("report txt", vec!["/h/u/Docs/report-2024.txt"]),
        // `|` ＝ 或，比空格紧：rs 且 (main 或 lib)。
        ("rs main|lib", vec!["/h/u/src/main.rs", "/h/u/src/lib.rs"]),
        ("main OR lib", vec!["/h/u/src/main.rs", "/h/u/src/lib.rs"]),
        // `!` ＝ 非。
        (
            ".rs !main",
            vec!["/h/u/src/lib.rs", "/h/u/src/report/mod.rs"],
        ),
        ("NOT report", {
            all.iter()
                .copied()
                .filter(|p| {
                    !matches!(
                        *p,
                        "/h/u/Docs/report.PDF" | "/h/u/Docs/report-2024.txt" | "/h/u/src/report"
                    )
                })
                .collect()
        }),
        // 通配：整个名字要对上。
        (
            "*.rs",
            vec![
                "/h/u/src/main.rs",
                "/h/u/src/lib.rs",
                "/h/u/src/report/mod.rs",
            ],
        ),
        ("?ain.rs", vec!["/h/u/src/main.rs"]),
        (
            "report*",
            vec![
                "/h/u/Docs/report.PDF",
                "/h/u/Docs/report-2024.txt",
                "/h/u/src/report",
            ],
        ),
        // `?` 是一个字符，不是一个字节。
        ("??.txt", vec!["/h/u/文档/报告.txt"]),
        // ext：只要文件，列表用 `;`，不分大小写，前导点可写可不写。
        (
            "ext:pdf;.TXT",
            vec![
                "/h/u/Docs/report.PDF",
                "/h/u/Docs/report-2024.txt",
                "/h/u/文档/报告.txt",
            ],
        ),
        ("ext:bashrc", vec!["/h/u/.bashrc"]),
        // path：对全路径。
        (
            "path:src/report",
            vec!["/h/u/src/report", "/h/u/src/report/mod.rs"],
        ),
        // 词里带分隔符 ⇒ 自动对全路径。
        (
            "docs/report",
            vec!["/h/u/Docs/report.PDF", "/h/u/Docs/report-2024.txt"],
        ),
        // file ／ folder。
        ("folder:", vec!["/h/u/Docs", "/h/u/src", "/h/u/src/report"]),
        ("folder:report", vec!["/h/u/src/report"]),
        (
            "file:report",
            vec!["/h/u/Docs/report.PDF", "/h/u/Docs/report-2024.txt"],
        ),
        // 引号里的空格照原样。
        ("\"summer holiday\"", vec!["/h/u/Docs/summer holiday.jpg"]),
        // 分组。
        (
            "<main|mod> .rs",
            vec!["/h/u/src/main.rs", "/h/u/src/report/mod.rs"],
        ),
        // 带大小写的非 ASCII 字母：按 Unicode 小写比。
        ("ärger", vec!["/h/u/Ärger.md"]),
        // 中文没有大小写：照字节比。
        ("报告", vec!["/h/u/文档/报告.txt"]),
        // 认不出的 `xx:` 当普通字。
        ("12:30", vec!["/h/u/12:30 notes"]),
        // 打到一半：没收口的引号 ／ 分组在末尾自动收口；`ext:` 还没写扩展名 ⇒ 先不缩。
        ("\"summer hol", vec!["/h/u/Docs/summer holiday.jpg"]),
        ("(main|lib", vec!["/h/u/src/main.rs", "/h/u/src/lib.rs"]),
        ("lib ext:", vec!["/h/u/src/lib.rs"]),
    ];
    for (q, want) in cases {
        assert_eq!(pick(q), set(want), "搜索词 `{q}` 挑出来的条目不对");
    }
}

#[test]
fn an_unsupported_filter_and_a_stray_closer_are_refused_by_name() {
    assert_eq!(
        parse("big size:>1mb"),
        Err(QueryError::Unsupported {
            name: "size".into()
        })
    );
    assert_eq!(
        parse("foo )"),
        Err(QueryError::StrayCloser { ch: ')', at: 5 })
    );
    // 两句都说得出那一处。
    assert!(parse("size:1").unwrap_err().said().contains("size"));
    assert!(parse("ab>").unwrap_err().said().contains('>'));
}
