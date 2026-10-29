//! 删掉的名字不许回来：`tests/__fixtures__/retired-names.tsv` 里到期的每个名字，仓里别处一处都不许有。
//!
//! 人群：`git ls-files` 的全部文本文件（与 `no_outside_refs` 同一口径），去掉本表与 `CHANGELOG.md`（版本记录照实写当时的名字）。
//! 表：一行「名字 ⇥ 删于」。「删于」空 ⇒ 今天起到期；写了版本（`X.Y` · `X.Y.Z`）⇒ 当前版本（`package.json` 的 `version`）
//! 到了那一版起到期：当桥的兼容代码挂下一版，打版本号那一刻当场红，由打版本号的那一路删。
//! 判定：到期的名字按标识符整词认（前后不紧挨字母、数字、`_`），命中集 == ∅；命中点名文件、行号与名字。
//! 删了一个符号、把提到它的散文也改成现状之后，想让它别回来 ⇒ 往表里加一行。说「它为什么不在了」的散文不写。
//!
//! 买不到：换了名字写回来的同一个东西；表里没登记的删名。

use std::collections::BTreeSet;

const TABLE: &str = "tests/__fixtures__/retired-names.tsv";
const EXEMPT: &[&str] = &[TABLE, "CHANGELOG.md"];

/// 版本号 `X.Y` · `X.Y.Z`（缺的那一段算 0）。
type Version = (u32, u32, u32);

fn parse_version(s: &str) -> Option<Version> {
    let parts: Vec<u32> = s
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    match parts[..] {
        [a, b] => Some((a, b, 0)),
        [a, b, c] => Some((a, b, c)),
        _ => None,
    }
}

/// 表里的一行：名字与「删于」（`None` ⇒ 今天起）。
struct Row {
    name: String,
    removed_in: Option<Version>,
}

/// 表里的行（`#` 打头的说明行由共享原语剥掉）；「删于」写坏了 ⇒ 点名那一行。
fn retired_rows(table: &str) -> Result<Vec<Row>, String> {
    crate::strip_hash_comment_lines(table)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (name, removed) = l.split_once('\t').unwrap_or((l, ""));
            let removed_in = match removed.trim() {
                "" => None,
                v => Some(parse_version(v).ok_or_else(|| format!("`{l}`：「删于」不是版本号"))?),
            };
            Ok(Row {
                name: name.trim().to_string(),
                removed_in,
            })
        })
        .collect()
}

/// 当前版本上到期的那几个名字：「删于」空，或 ≤ 当前版本。
fn due_names(rows: &[Row], current: Version) -> Vec<&str> {
    rows.iter()
        .filter(|r| r.removed_in.is_none_or(|v| v <= current))
        .map(|r| r.name.as_str())
        .collect()
}

/// `package.json` 的 `version`（打版本号改的就是它）。
fn current_version(package_json: &str) -> Option<Version> {
    let re = regex::Regex::new(r#""version"\s*:\s*"([0-9.]+)""#).ok()?;
    parse_version(re.captures(package_json)?.get(1)?.as_str())
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
    let rows = retired_rows(&table).unwrap_or_else(|e| panic!("禁名表坏了：{e}"));
    let unique: BTreeSet<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert!(
        rows.len() > 100 && unique.len() == rows.len(),
        "禁名表 {} 行、去重后 {} 个 —— 表坏了或有重复行",
        rows.len(),
        unique.len()
    );
    let pkg = std::fs::read_to_string(root.join("package.json")).expect("package.json 读不动");
    let current = current_version(&pkg).expect("package.json 里读不出 version");
    let names = due_names(&rows, current);
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
    let rows = retired_rows(&table).expect("合成表读不进来");
    assert_eq!(
        due_names(&rows, (5, 0, 3)),
        vec![n.as_str()],
        "说明行与空行没被跳过"
    );
}

/// 合成夹具：「删于」空 ⇒ 到期；≤ 当前版本 ⇒ 到期；> 当前版本 ⇒ 还在当桥；写坏了 ⇒ 表读不进来。
#[test]
fn a_removed_in_version_is_due_from_that_version_on() {
    let table =
        "# 说明\nnow_gone\nbridge_a\t5.0.4\nbridge_b\t5.1\nold_bridge\t5.0.3\nspaced name\t\n";
    let rows = retired_rows(table).expect("合成表读不进来");
    assert_eq!(
        due_names(&rows, (5, 0, 3)),
        vec!["now_gone", "old_bridge", "spaced name"],
        "5.0.3 上到期的 ≠ 标定（空 · 5.0.3 到期；5.0.4 · 5.1 还在当桥）"
    );
    assert_eq!(
        due_names(&rows, (5, 0, 4)),
        vec!["now_gone", "bridge_a", "old_bridge", "spaced name"],
        "版本号推到 5.0.4，挂 5.0.4 的那一行没到期"
    );
    assert_eq!(due_names(&rows, (5, 1, 0)).len(), 5, "推到 5.1 全部到期");
    assert!(
        retired_rows("x\tnext").is_err(),
        "「删于」写成不是版本号的样子没被拒"
    );
    assert_eq!(
        current_version("{\n  \"name\": \"x\",\n  \"version\": \"5.0.3\"\n}"),
        Some((5, 0, 3))
    );
}

/// 手机端的旧产品名：拆两段拼，本文件里没有一处完整的字面。
fn old_mobile_name() -> String {
    ["a", "term"].concat()
}

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
