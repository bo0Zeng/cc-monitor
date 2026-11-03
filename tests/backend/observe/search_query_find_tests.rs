//! 会话内查找（`--find-in-session`）的判据。
//!
//! 买到：命中集合两向（期望取自夹具的 uuid 命名 `hit-*` / `miss-*`，不取自判定函数）· 文件序 ·
//! `--include-tools` 两态 · 上限只砍「列」不砍「数」· 与 `--search` 对同一份文件给出同一组命中与片段 ·
//! argv 的写法与报错形 · 分派与路径守卫。
//! **买不到**：命中之后前端跳不跳得到那张卡（工具结果并进工具组时找不到 `[data-id]`）—— 那在前端判。
//! 夹具是**合成的结构**，不含任何真会话正文。

use super::test_support::{bytes_of, find, fixture, uuids, Q};
use super::*;

/// 夹具里按命名该中的那几条（文件序）。
fn named(lines: &[String], prefixes: &[&str]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| {
            let i = l.find("\"uuid\":\"")? + 8;
            let u = &l[i..i + l[i..].find('"')?];
            prefixes
                .iter()
                .any(|p| u.starts_with(p))
                .then(|| u.to_string())
        })
        .collect()
}

/// 🔴 B3 主判据：命中的 uuid 序列 **==** 夹具里 `hit-*`（不带工具）/ `hit-*`＋`tool-*`（带工具），文件序，两向。
/// 反空真：三类（hit / tool / miss）各自非空；带不带工具两态的期望**不相等**（否则那个开关没被判到）。
#[test]
fn the_hits_are_exactly_the_named_ones_in_file_order() {
    let lines = fixture();
    let data = bytes_of(&lines);
    let plain = named(&lines, &["hit-"]);
    let with_tools = named(&lines, &["hit-", "tool-"]);
    assert_eq!(plain.len(), 5, "夹具里 hit-* 的条数变了：{plain:?}");
    assert_eq!(with_tools.len(), 7);
    assert!(named(&lines, &["miss-"]).len() >= 5, "miss-* 一类塌了");
    let v = find(&data, Q, false, FIND_DEFAULT_LIMIT);
    assert_eq!(uuids(&v), plain);
    assert_eq!(v[v.len() - 1]["total"], plain.len() as u64);
    let v = find(&data, Q, true, FIND_DEFAULT_LIMIT);
    assert_eq!(uuids(&v), with_tools);
    // 种类同 `Hit.kind` 那套词
    let kinds: Vec<&str> = v[1..v.len() - 1]
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "user",
            "assistant",
            "tool",
            "tool",
            "user",
            "report",
            "assistant"
        ],
        "{kinds:?}"
    );
}

/// 片段三段：原文里的大小写原样保留在 `matched`，前后文是命中点两侧的原文。
#[test]
fn snippets_keep_the_original_case() {
    let v = find(&bytes_of(&fixture()), Q, false, FIND_DEFAULT_LIMIT);
    let r = &v[2]; // hit-asst-upper
    assert_eq!(r["uuid"], "hit-asst-upper");
    assert_eq!(r["matched"], "NEEDLE");
    // 前后文（`make_snippet` 会折叠 / 修边空白）
    assert_eq!(r["before"].as_str().unwrap().trim(), "a");
    assert_eq!(r["after"].as_str().unwrap().trim(), "in caps");
}

/// B5：上限只砍「列」不砍「数」—— `count == min(limit, total)`、`total` 恒为全量；列出来的是**最前面**那几条。
#[test]
fn the_limit_cuts_the_list_not_the_count() {
    let lines = fixture();
    let data = bytes_of(&lines);
    let all = named(&lines, &["hit-"]);
    for limit in [0usize, 1, 3, 4, 9] {
        let v = find(&data, Q, false, limit);
        let tail = &v[v.len() - 1];
        assert_eq!(tail["total"], all.len() as u64, "limit={limit}");
        assert_eq!(tail["count"], limit.min(all.len()) as u64, "limit={limit}");
        assert_eq!(uuids(&v), all[..limit.min(all.len())].to_vec());
    }
}

/// 空查询 / 全空白 ⇒ 零条（头尾照出）；torn 残尾不看。
#[test]
fn blank_query_and_torn_tail_yield_nothing_extra() {
    let data = bytes_of(&fixture());
    for q in ["", "   "] {
        let v = find(&data, q, true, FIND_DEFAULT_LIMIT);
        assert_eq!(v.len(), 2, "空查询只有头尾");
        assert_eq!(v[1]["total"], 0);
    }
    let mut torn = data.clone();
    torn.extend_from_slice(br#"{"type":"user","uuid":"hit-torn","message":{"content":"needle"}}"#);
    assert!(!uuids(&find(&torn, Q, false, FIND_DEFAULT_LIMIT)).contains(&"hit-torn".to_string()));
}

/// 🔴 B4：与 `--search` **同一份口径**的行为面 —— 对同一份文件、同一查询，
/// `--search`（`session_hits_in`）给的 uuid 序列与三段片段，与本命令逐条相等（上限之内）。
/// 两个出口各跑各的管线（一个走全局预算、带时间戳；一个按文件序、不带），只在「口径」上必须重合。
#[test]
fn find_and_global_search_agree_on_the_same_file() {
    let tmp = std::env::temp_dir().join(format!("ccm-find-vs-search-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let p = tmp.join("s.jsonl");
    std::fs::write(&p, bytes_of(&fixture())).unwrap();
    for tools in [false, true] {
        let opts = SearchOpts {
            include_tools: tools,
            scope: None,
            after_ms: 0,
            limit: crate::history::search_rules::DEFAULT_LIMIT,
            titles: false,
        };
        let mut budget = SnippetBudget::new(opts.limit);
        let q = Q.trim().to_lowercase();
        // 会话那一格来自索引：整份读进一格 `FileEntry`。
        let mut entry = FileEntry::empty(crate::agents::claudecode::AGENT_KIND, true);
        entry
            .take(
                None,
                &mut Blocks::new(&std::fs::read(&p).expect("读夹具")[..], 1 << 20),
            )
            .expect("读得动");
        let s = session_hits_in(&p, &entry, &q, &opts, &mut budget, 0).expect("有命中");
        // 两者刻意的差别两处：`--search` 也列没有 uuid 的记录（uuid 记成空串）、本命令不列；`--search` 每个会话只给前几处。
        //   agent 回报（种类 `report`）两边都收。
        let searched: Vec<(String, String, String, String)> = s
            .hits
            .iter()
            .filter(|h| !h.uuid.is_empty())
            .map(|h| {
                (
                    h.uuid.clone(),
                    h.before.clone(),
                    h.matched.clone(),
                    h.after.clone(),
                )
            })
            .collect();
        let v = find(&bytes_of(&fixture()), Q, tools, FIND_DEFAULT_LIMIT);
        let found: Vec<(String, String, String, String)> = v[1..v.len() - 1]
            .iter()
            .map(|h| {
                let g = |k: &str| h[k].as_str().unwrap().to_string();
                (g("uuid"), g("before"), g("matched"), g("after"))
            })
            .collect();
        assert!(!searched.is_empty());
        assert_eq!(found[..searched.len()], searched[..], "tools={tools}");
    }
    std::fs::remove_dir_all(&tmp).ok();
}

// 帧面 `history-find`（常驻索引那一臂 == 现扫 · 追加之后只读尾巴 · 第几轮与时刻 · 只搜用户）那几条要帧面宿主 ⇒ 挂主 crate：
// `search_query_find_on_upper_tests.rs`。

/// ★ 查找的单位就是流里画的那几张卡：工具结果流里不自己成卡、收进它那次工具调用的卡（派出子运行的结果收进派出卡），
/// 核心在命中上说它落在哪次调用的卡里（`tool`），界面照它跳、不自己推。别的命中落在自己那张卡上 ⇒ 没有 `tool`。
#[test]
fn a_tool_result_hit_says_which_tool_card_it_lands_in() {
    let data = bytes_of(&fixture());
    let v = find(&data, Q, true, 100);
    let tool_of = |uuid: &str| {
        v.iter()
            .find(|h| h["uuid"] == uuid)
            .unwrap_or_else(|| panic!("{uuid} 该中"))
            .get("tool")
            .cloned()
    };
    assert_eq!(tool_of("tool-result"), Some(serde_json::json!("t")));
    assert_eq!(tool_of("tool-use-input"), None);
    assert_eq!(tool_of("hit-user-str"), None);
}

/// ★ 命中带行号：会话内查找（现扫那一臂；常驻索引那一臂经帧面问、挂主 crate）与全局搜索的每条命中，行号 ＝ 那一条在扫描图骨架索引里的下标
/// （夹具里有坏行 · 空行 · 带 BOM 的行，行号与物理行号不同）。
#[test]
fn hits_carry_the_index_row_of_their_record() {
    let home = std::env::temp_dir().join(format!("find-line-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("p");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let data = bytes_of(&fixture());
    std::fs::write(&path, &data).unwrap();
    let map = crate::observe::record_scan::ScanMap::scan_with(
        &data[..],
        data.len() as u64,
        crate::agents::claudecode::AGENT_KIND,
        None,
    )
    .unwrap();
    let row_of = |u: &str| {
        map.index
            .iter()
            .position(|r| r.u.as_deref() == Some(u))
            .map(|i| i as u64)
    };
    // `ctl`：要不要正控（有行号与物理行号不同的那一条）——全局搜索每会话只给前 3 处，夹具前 3 处恰在坏行之前，那一臂只核相等。
    let check = |what: &str, got: Vec<(String, u64)>, ctl: bool| {
        assert!(got.len() >= 3, "{what}：夹具该有几条命中");
        let want: Vec<(String, u64)> = got
            .iter()
            .map(|(u, _)| (u.clone(), row_of(u).expect("命中那一条不在索引里")))
            .collect();
        assert_eq!(got, want, "{what}：命中带的行号与骨架索引里的下标不一样");
        let physical = |u: &str| {
            fixture()
                .iter()
                .position(|l| l.contains(&format!("\"uuid\":\"{u}\"")))
                .unwrap() as u64
        };
        assert!(
            !ctl || got.iter().any(|(u, l)| physical(u) != *l),
            "{what}：正控，要有行号与物理行号不同的"
        );
    };
    let pairs = |v: &serde_json::Value| -> Vec<(String, u64)> {
        v["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                (
                    h["uuid"].as_str().unwrap().to_string(),
                    h["line"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let scanned = find(&data, Q, true, FIND_DEFAULT_LIMIT);
    check(
        "现扫",
        pairs(&serde_json::json!({ "hits": scanned[1..scanned.len() - 1] })),
        true,
    );
    let opts = SearchOpts {
        include_tools: true,
        scope: None,
        after_ms: 0,
        limit: crate::history::search_rules::DEFAULT_LIMIT,
        titles: false,
    };
    let mut entry = FileEntry::empty(crate::agents::claudecode::AGENT_KIND, true);
    entry
        .take(None, &mut Blocks::new(&data[..], 7))
        .expect("读得动");
    let s = session_hits_in(
        &path,
        &entry,
        &Q.to_lowercase(),
        &opts,
        &mut SnippetBudget::new(opts.limit),
        0,
    )
    .expect("有命中");
    check(
        "全局搜索",
        s.hits
            .iter()
            .filter(|h| !h.uuid.is_empty())
            .map(|h| (h.uuid.clone(), h.line))
            .collect(),
        false,
    );
    let _ = std::fs::remove_dir_all(&home);
}
