//! 〔SE2 · `设计/10 §6 步 6`〕会话内查找（`--find-in-session`）的判据。
//!
//! 买到：命中集合两向（期望取自夹具的 uuid 命名 `hit-*` / `miss-*`，不取自判定函数）· 文件序 ·
//! `--include-tools` 两态 · 上限只砍「列」不砍「数」· 与 `--search` 对同一份文件给出同一组命中与片段 ·
//! argv 的写法与报错形 · 分派与路径守卫。
//! **买不到**：命中之后前端跳不跳得到那张卡（工具结果并进工具组时找不到 `[data-uuid]`）—— 那在前端判。
//! 夹具是**合成的结构**，不含任何真会话正文。

use super::*;

const Q: &str = "NeEdLe";

/// 每行一个形状，`uuid` 自带「该不该中」：`hit-*` 不看工具也中 · `tool-*` 只在带工具时中 · `miss-*` 永不中。
fn fixture() -> Vec<String> {
    vec![
        r#"{"type":"user","uuid":"hit-user-str","message":{"content":"find the needle here"}}"#.into(),
        r#"{"type":"assistant","uuid":"miss-plain","message":{"content":[{"type":"text","text":"nothing to see"}]}}"#.into(),
        r#"{"type":"assistant","uuid":"hit-asst-upper","message":{"content":[{"type":"text","text":"a NEEDLE in caps"}]}}"#.into(),
        // 注入的包装在 user 正文里被剥掉（`clean_user_text`）⇒ 只在包装里出现的不算
        r#"{"type":"user","uuid":"miss-wrapped","message":{"content":"<system-reminder>needle</system-reminder>real words"}}"#.into(),
        r#"{"type":"assistant","uuid":"tool-use-input","message":{"content":[{"type":"tool_use","name":"Grep","input":{"pattern":"needle"}}]}}"#.into(),
        r#"{"type":"user","uuid":"tool-result","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"line with needle"}]}}"#.into(),
        // 没有 uuid / 空 uuid ⇒ 跳不过去 ⇒ 不列
        r#"{"type":"user","message":{"content":"needle without uuid"}}"#.into(),
        r#"{"type":"user","uuid":"","message":{"content":"needle with empty uuid"}}"#.into(),
        // 不是 user/assistant ⇒ 不搜
        r#"{"type":"system","uuid":"miss-system","content":"needle in system"}"#.into(),
        r#"{"type":"ai-title","uuid":"miss-title","aiTitle":"needle title"}"#.into(),
        // 非 JSON
        r#"{"type":"user","uuid":"miss-broken","message":{"content":"needle"#.into(),
        "".into(),
        "\u{feff}{\"type\":\"user\",\"uuid\":\"hit-bom\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"x needle y\"}]}}".into(),
        r#"{"type":"user","uuid":"hit-sidechain","isSidechain":true,"message":{"content":"sub needle"}}"#.into(),
    ]
}

fn bytes_of(lines: &[String]) -> Vec<u8> {
    let mut v = Vec::new();
    for l in lines {
        v.extend_from_slice(l.as_bytes());
        v.push(b'\n');
    }
    v
}

fn find(data: &[u8], q: &str, tools: bool, limit: usize) -> Vec<Value> {
    let mut out = Vec::new();
    write_session_find(data, q, tools, limit, &mut out).expect("find ok");
    let v: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("每行都是 JSON"))
        .collect();
    assert_eq!(v[0]["kind"], "session_find", "首行认得出");
    assert_eq!(v[v.len() - 1]["kind"], "session_find_end", "有尾行");
    assert_eq!(
        v[v.len() - 1]["count"].as_u64().unwrap() as usize,
        v.len() - 2,
        "尾行条数 == 实到条数"
    );
    v
}

fn uuids(v: &[Value]) -> Vec<String> {
    v[1..v.len() - 1]
        .iter()
        .map(|r| r["uuid"].as_str().unwrap().to_string())
        .collect()
}

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
    assert_eq!(plain.len(), 4, "夹具里 hit-* 的条数变了：{plain:?}");
    assert_eq!(with_tools.len(), 6);
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
        ["user", "assistant", "tool", "tool", "user", "user"],
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
            limit: search_core::DEFAULT_LIMIT,
        };
        let mut budget = SnippetBudget::new(opts.limit);
        let q = Q.trim().to_lowercase();
        // 〔SX1〕会话那一格来自索引：整份读进一格 `FileEntry`。
        let mut entry = FileEntry::empty(None, true);
        entry.take(None, &std::fs::read(&p).expect("读夹具"));
        let s = session_hits_in(&p, &entry, &q, &opts, &mut budget, 0).expect("有命中");
        // `--search` 也列没有 uuid 的记录（uuid 记成空串）；本命令不列 —— 这是两者**唯一**刻意的差别
        let searched: Vec<(String, String, String, String)> = s["hits"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| !h["uuid"].as_str().unwrap().is_empty())
            .map(|h| {
                let g = |k: &str| h[k].as_str().unwrap().to_string();
                (g("uuid"), g("before"), g("matched"), g("after"))
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
        assert!(!found.is_empty());
        assert_eq!(found, searched, "tools={tools}");
    }
    std::fs::remove_dir_all(&tmp).ok();
}

/// ★ 〔GAP1 · `设计/10 §7` 第 10 条〕「会话内查找每次从头扫一遍文件」⇒ `history-find` 走 SX1 常驻索引：
/// 应答 == 现扫（`scan_session_find`，两态 `include_tools`；torn 残尾与没 uuid 的行照样不列），追加之后那一问只读尾巴。
#[test]
fn gap1_history_find_rides_the_resident_index_and_equals_the_plain_scan() {
    let home = std::env::temp_dir().join(format!("gap1-find-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("p");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let rec = |uuid: &str, text: &str| {
        serde_json::json!({"type":"user","uuid":uuid,"message":{"content":[{"type":"text","text":text},
            {"type":"tool_result","tool_use_id":"t","content":format!("tool {text}")}]}})
        .to_string()
    };
    let mut body = [
        rec("a", "zqx one"),
        rec("", "zqx no uuid"),
        rec("b", "plain"),
    ]
    .join("\n");
    body.push('\n');
    body.push_str(&rec("torn", "zqx torn")); // 没换行 ⇒ 不列
    std::fs::write(&path, &body).unwrap();
    let p = path.display().to_string();
    let scan = |tools: bool| {
        let r = crate::observe::history_query::open_session_at(&home, &p, 0).unwrap();
        let mut hits = Vec::new();
        let (_, total) =
            crate::observe::search_query::scan_session_find(r, "zqx", tools, 500, |h| {
                hits.push(h.clone());
                Ok(())
            })
            .unwrap();
        serde_json::json!({ "total": total, "hits": hits })
    };
    let ask = |tools: bool| {
        crate::faces::read_face::answer_at(
            &home,
            "history-find",
            &serde_json::json!({"path": p, "query": "zqx", "include_tools": tools, "limit": 500}),
        )
        .unwrap()
    };
    assert_eq!(ask(false), scan(false));
    assert_eq!(ask(true), scan(true));
    // 残尾补完 ＋ 再追加一行 ⇒ 这一问只追加读（不整份重读），应答仍 == 现扫。
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    use std::io::Write as _;
    writeln!(f, "\n{}", rec("c", "zqx later")).unwrap();
    drop(f);
    assert_eq!(ask(true), scan(true));
    let root = Fence::at(&projects_root(&home))
        .unwrap()
        .root()
        .to_path_buf();
    let last = RESIDENT.lock().unwrap().get(&root).map(|i| i.last);
    assert_eq!(
        last.map(|r| (r.full, r.appended, r.reused)),
        Some((0, 1, 0)),
        "(整份, 追加, 没读)：该只追加读这一份"
    );
    let _ = std::fs::remove_dir_all(&home);
}
