//! 删掉的名字不许回来：`tests/__fixtures__/retired-names.tsv` 里的每个名字，仓里别处一处都不许有。
//!
//! 人群：`git ls-files` 的全部文本文件（与 `no_outside_refs` 同一口径），去掉本表与 `CHANGELOG.md`（版本记录照实写当时的名字）。
//! 判定：按标识符整词认（前后不紧挨字母、数字、`_`），命中集 == ∅；命中点名文件、行号与名字。
//! 删了一个符号、把提到它的散文也改成现状之后，想让它别回来 ⇒ 往表里加一行。说「它为什么不在了」的散文不写。
//!
//! 买不到：换了名字写回来的同一个东西；表里没登记的删名。

use std::collections::BTreeSet;

const TABLE: &str = "tests/__fixtures__/retired-names.tsv";
const EXEMPT: &[&str] = &[TABLE, "CHANGELOG.md"];

/// 表里的名字（`#` 打头的说明行由共享原语剥掉）。
fn retired_names(table: &str) -> Vec<String> {
    crate::strip_hash_comment_lines(table)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `line` 里整词出现的第一个表中名字。
fn first_retired_in<'a>(names: &[&'a str], line: &str) -> Option<&'a str> {
    names.iter().copied().find(|n| {
        line.match_indices(n).any(|(at, _)| {
            let before = line[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !ident_char(c));
            let after = line[at + n.len()..]
                .chars()
                .next()
                .is_none_or(|c| !ident_char(c));
            before && after
        })
    })
}

#[test]
fn no_retired_name_comes_back() {
    let root = super::no_outside_refs::repo();
    let table = std::fs::read_to_string(root.join(TABLE)).expect("禁名表读不动");
    let owned = retired_names(&table);
    let names: Vec<&str> = owned.iter().map(String::as_str).collect();
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert!(
        names.len() > 100 && unique.len() == names.len(),
        "禁名表 {} 行、去重后 {} 个 —— 表坏了或有重复行",
        names.len(),
        unique.len()
    );
    let texts = super::no_outside_refs::tracked_texts(&root);
    assert!(
        texts.len() > 500,
        "人群只有 {} 份 —— `git ls-files` 口径坏了，零命中不作数",
        texts.len()
    );
    let mut hits = Vec::new();
    for (rel, text) in &texts {
        if EXEMPT.contains(&rel.as_str()) {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            if let Some(n) = first_retired_in(&names, line) {
                hits.push(format!("{rel}:{}: `{n}`", i + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "删掉的名字又出现了 {} 处（禁名表 `{TABLE}`）：代码里回来了就换个名字或别回来；\
         散文里提着就改成说现状。\n{}",
        hits.len(),
        hits.join("\n")
    );
}

/// 合成夹具：整词命中必须抓到，粘在别的标识符里的必须放过（行号集相等）。
#[test]
fn the_retired_name_match_is_whole_word() {
    let n = format!("zz{u}gone{u}name", u = "_");
    let names = [n.as_str()];
    let lines = [
        format!("fn {n}() {{}}"),             // 1 ✔ 代码
        format!("/// 由 `{n}` 钉住"),         // 2 ✔ 散文
        format!("let x = {n}_v2;"),           // 3 ✘ 后面粘着
        format!("let x = a{n};"),             // 4 ✘ 前面粘着
        format!("\"{n}\","),                  // 5 ✔ 字符串
        "fn something_else() {}".to_string(), // 6 ✘
    ];
    let got: BTreeSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| first_retired_in(&names, l).is_some())
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        got,
        BTreeSet::from([1, 2, 5]),
        "整词匹配在合成夹具上抓到的行 ≠ 标定（该抓 1、2、5；该放过 3、4、6）—— 匹配坏了，全仓那条零命中不作数"
    );
    let table = format!("# 说明\n\n{n}\n");
    assert_eq!(
        retired_names(&table),
        vec![n.as_str()],
        "说明行与空行没被跳过"
    );
}

/// 手机端的旧产品名：拆两段拼，本文件里没有一处完整的字面。
fn old_mobile_name() -> String {
    ["a", "term"].concat()
}

/// 手机那棵树里留着旧名的唯一一份：它钉的就是「远端那层旧目录的字面不许回来」，必须写出那两个字面。
const OLD_MOBILE_NAME_PIN: &str =
    "src/mobile/app/src/test/kotlin/com/ccmonitor/mobile/ssh/NamespaceLiteralScanTest.kt";

/// `line` 里有没有旧名（不分大小写；`highwatermark` 这个词里恰好带着它，不算）。
fn says_old_mobile_name(line: &str, old: &str) -> bool {
    line.to_lowercase()
        .replace("highwatermark", "")
        .contains(old)
}

/// 手机那棵树（`src/mobile/`，代码 · 资源 · 脚本 · 文档一起）里不许再有旧产品名：
/// 类名 · 主题名 · 标记 · 环境变量 · 落盘名 · 文案都叫 cc-monitor 这一族。
#[test]
fn the_mobile_tree_never_says_the_old_product_name() {
    let root = super::no_outside_refs::repo();
    let texts = super::no_outside_refs::tracked_texts(&root);
    let old = old_mobile_name();
    let mobile: Vec<&(String, String)> = texts
        .iter()
        .filter(|(rel, _)| rel.starts_with("src/mobile/"))
        .collect();
    assert!(
        mobile.len() > 300,
        "手机那棵树只扫到 {} 份 —— 口径坏了，零命中不作数",
        mobile.len()
    );
    let mut hits = Vec::new();
    for (rel, text) in mobile {
        if rel == OLD_MOBILE_NAME_PIN {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            if says_old_mobile_name(line, &old) {
                hits.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "手机那棵树里还有 {} 处旧产品名：改成 cc-monitor 这一族（类名 CcMonitor… · 标记 CCM_… · 落盘名 ccm…）。\n{}",
        hits.len(),
        hits.join("\n")
    );
}

/// 合成夹具：大小写都认，`highwatermark` 放过。
#[test]
fn the_old_mobile_name_match_ignores_case_and_spares_highwatermark() {
    let old = old_mobile_name();
    let up = old.to_uppercase();
    let lines = [
        format!("class {}App : Application()", "A".to_string() + &old[1..]), // 1 ✔
        format!("const val X = \"{up}_ADDED\""),                             // 2 ✔
        format!(".databaseBuilder(ctx, Db::class.java, \"{old}.db\")"),      // 3 ✔
        "// `.lock` / `.highwatermark` 忽略".to_string(),                    // 4 ✘
        "class CcMonitorApp : Application()".to_string(),                    // 5 ✘
    ];
    let got: BTreeSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| says_old_mobile_name(l, &old))
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        got,
        BTreeSet::from([1, 2, 3]),
        "旧名匹配在合成夹具上抓到的行 ≠ 标定（该抓 1–3；该放过 4、5）—— 匹配坏了，手机那条零命中不作数"
    );
}
