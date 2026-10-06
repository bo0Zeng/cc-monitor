use super::*;

// ⚠ `extract_*` / `user_text` / `find_ci` / `make_snippet` 那 4 条单元测试
// **随实现住在口径的家里**（`K-R100`；通用那几条在 `search_rules_tests.rs`，记录文本那几条在 `agents/claudecode/text_tests.rs`）。
// 在这里再抄一份 = 又在本文件养出一个「口径的家」，正是本件要治的形状。

#[test]
fn parse_iso8601_basic() {
    // 1970-01-01T00:00:00Z = 0
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
    // 1970-01-01T00:00:01.500Z = 1500
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.500Z"), Some(1500));
    // 小数秒变体归一到毫秒：.12 → 120ms，.1 → 100ms，.123456 → 123ms，无小数 → 0
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.12Z"), Some(120));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.1Z"), Some(100));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.123456Z"), Some(123));
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
    // 2021-01-01T00:00:00Z = 1609459200000
    assert_eq!(
        parse_iso8601_ms("2021-01-01T00:00:00Z"),
        Some(1_609_459_200_000)
    );
    assert_eq!(parse_iso8601_ms("garbage"), None);
}

#[test]
fn search_end_to_end_and_rejects_traversal() {
    let tmp = std::env::temp_dir().join(format!("ccm-search-test-{}", std::process::id()));
    let proj = tmp.join("projects").join("proj-a");
    std::fs::create_dir_all(&proj).unwrap();
    let jsonl = proj.join("s1.jsonl");
    std::fs::write(
        &jsonl,
        [
            r#"{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","cwd":"/home/pi/proj","message":{"role":"user","content":"请用 Docker 部署"}}"#,
            r#"{"type":"assistant","uuid":"a1","timestamp":"2026-01-01T00:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"好的，用 docker compose"}]}}"#,
        ]
        .join("\n"),
    )
    .unwrap();

    let opts = SearchOpts {
        include_tools: false,
        scope: None,
        after_ms: 0,
        limit: 300,
        titles: false,
    };
    // 会话那一格来自索引：整份读进一格 `FileEntry`，查询对它跑。
    let mut entry = FileEntry::empty(None, true);
    entry.take(None, &std::fs::read(&jsonl).expect("读夹具"));
    let mut budget = SnippetBudget::new(opts.limit);
    let hit = session_hits_in(
        &jsonl,
        &entry,
        "docker",
        &opts,
        &mut budget,
        1_700_000_000_000,
    )
    .expect("must hit");
    assert_eq!(hit["sessionId"], "s1");
    assert_eq!(
        hit["updatedAt"], 1_700_000_000_000i64,
        "updatedAt 用调用方传进来的那份"
    );
    assert_eq!(hit["projectPath"], "/home/pi/proj");
    assert_eq!(
        hit["hitCount"], 2,
        "user + assistant both match 'docker' ci"
    );
    assert!(hit["hits"].as_array().unwrap().len() == 2);
    assert_eq!(
        hit["hitsTruncated"], false,
        "预算充足、两条都给了 snippet ⇒ 没被截断"
    );

    // scope=user → 只 user 命中
    let opts_u = SearchOpts {
        include_tools: false,
        scope: Some("user".into()),
        after_ms: 0,
        limit: 300,
        titles: false,
    };
    let mut budget2 = SnippetBudget::new(opts_u.limit);
    let hu =
        session_hits_in(&jsonl, &entry, "docker", &opts_u, &mut budget2, 1).expect("user hits");
    assert_eq!(hu["hitCount"], 1);

    std::fs::remove_dir_all(&tmp).ok();
}

/// 设计稿「文件与历史」乙6（PARSE 10-02 定）：agent 回报（子 agent 交回 / 发来的话 · 另一个会话发来的话）搜得到，
/// 命中种类单列 `report`；`scope` 认 `report`（只要它）、`user` 不含它（只要人说的）。正控：人说的那一条照旧是 `user`。
#[test]
fn agent_reports_are_searchable_as_their_own_kind() {
    let lines = [
        r#"{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","cwd":"/p","message":{"role":"user","content":"部署脚本要加回滚"}}"#,
        r#"{"type":"user","uuid":"r1","timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":"<agent-message from=\"a1\">回滚已加好，部署脚本测过了</agent-message>"}}"#,
        r#"{"type":"user","uuid":"p1","timestamp":"2026-01-01T00:00:02Z","message":{"role":"user","content":"<cross-session-message from=\"s9\">那边的部署脚本也要改</cross-session-message>"}}"#,
    ]
    .join("\n");
    let mut entry = FileEntry::empty(None, true);
    entry.take(None, lines.as_bytes());
    let path = std::path::Path::new("/x/projects/p/s1.jsonl");
    let ask = |scope: Option<&str>| {
        let opts = SearchOpts {
            include_tools: false,
            scope: scope.map(str::to_string),
            after_ms: 0,
            limit: 300,
            titles: false,
        };
        let mut b = SnippetBudget::new(opts.limit);
        session_hits_in(path, &entry, "部署脚本", &opts, &mut b, 1)
    };
    let all = ask(None).expect("三条都命中");
    let kinds: Vec<&str> = all["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["user", "report", "report"]);
    assert_eq!(ask(Some("report")).unwrap()["hitCount"], 2);
    assert_eq!(ask(Some("user")).unwrap()["hitCount"], 1, "user 只要人说的");
    assert!(ask(Some("assistant")).is_none());
}

/// 系统注入（`isMeta` 的那一条 · 字全是注入的那一条）搜不到：界面「显示系统注入」关着时它不在任何读路径上，
/// 开着也只是排版（会话内查找不分开关，一律不搜注入）。含工具内容也一样。正控：人说的那一条照旧命中。
#[test]
fn system_injections_are_never_searched() {
    let lines = [
        r#"{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","cwd":"/p","message":{"role":"user","content":"查一下注入词甲"}}"#,
        r#"{"type":"user","uuid":"m1","timestamp":"2026-01-01T00:00:01Z","isMeta":true,"message":{"role":"user","content":"注入词乙只在这里"}}"#,
        r#"{"type":"user","uuid":"m2","timestamp":"2026-01-01T00:00:02Z","message":{"role":"user","content":"<system-reminder>注入词丙只在这里</system-reminder>"}}"#,
    ]
    .join("\n");
    let mut entry = FileEntry::empty(None, true);
    entry.take(None, lines.as_bytes());
    let path = std::path::Path::new("/x/projects/p/s1.jsonl");
    let ask = |q: &str, include_tools: bool| {
        let opts = SearchOpts {
            include_tools,
            scope: None,
            after_ms: 0,
            limit: 300,
            titles: false,
        };
        let mut b = SnippetBudget::new(opts.limit);
        session_hits_in(path, &entry, q, &opts, &mut b, 1)
    };
    for tools in [false, true] {
        assert!(ask("注入词乙", tools).is_none(), "isMeta 那一条不搜");
        assert!(ask("注入词丙", tools).is_none(), "全是注入的那一条不搜");
        assert_eq!(ask("注入词甲", tools).unwrap()["hitCount"], 1);
    }
}

// ── `K-R100` 的三条行为判据 ──────────────────────────────────────────
// 它们**不判源码文本**（那是判写法，且今天两侧本来就一样，会恒绿）。
// 判的是「本侧真跑出来的东西跟不跟 `search_rules` 走」。

/// 建一棵 `<home>/projects/<proj>/<sid>.jsonl` 语料，`mtimes` 按给定毫秒设。
fn corpus(tag: &str, sessions: &[(&str, i64, usize)]) -> std::path::PathBuf {
    let tmp = std::env::temp_dir().join(format!("ccm-kr100-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&tmp).ok();
    for (sid, mtime_ms_val, n_hits) in sessions {
        let proj = tmp.join("projects").join(format!("p-{sid}"));
        std::fs::create_dir_all(&proj).unwrap();
        let jsonl = proj.join(format!("{sid}.jsonl"));
        let lines: Vec<String> = (0..*n_hits)
            .map(|i| {
                format!(
                    r#"{{"type":"user","uuid":"{sid}-{i}","timestamp":"2026-01-01T00:00:0{}Z","cwd":"/w","message":{{"role":"user","content":"命中 docker 第 {i} 条"}}}}"#,
                    i % 10
                )
            })
            .collect();
        std::fs::write(&jsonl, lines.join("\n")).unwrap();
        let t = filetime_from_ms(*mtime_ms_val);
        set_mtime(&jsonl, t);
    }
    tmp
}
fn filetime_from_ms(ms: i64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64)
}
/// 只用 std 设 mtime（本 crate 不引 `filetime`）。
///
/// 🔴 `K-R122`（09-14）：**这一处换成了 `std::fs::File::set_times`，加的不是 `cfg`。**
/// 上一版走 `unsafe { libc::utimensat(libc::AT_FDCWD, …) }` —— 那两个名字在
/// `x86_64-pc-windows-msvc` 上**不存在**（`libc` 的 Windows 侧没有它们），
/// 于是后端那条「Windows 编得过」的跨 target check 在 **test 档**上红了 2 个错。
///
/// ⚠ **为什么这一处与 `sidecars/codepicture/acquire.rs` 那三条的处置相反**：
/// 那三条断的是**只在 unix 上成立的语义**（可执行位 · `chmod` 造出来的 `EACCES`），
/// 换个平台连前提都不成立 ⇒ 加 `cfg`；而**「把一份文件的 mtime 设成某个值」在
/// Windows 上照样成立**，缺的只是一条跨平台的写法 —— `std` 从 1.75 起就有
/// （[`std::fs::FileTimes`]）。⇒ 这一处该换 API，不该加 `cfg`：加了 `cfg`
/// 就等于把「预算按最近优先花」那一族判据在 Windows 上整族关掉，而它们本来跑得了。
///
/// ⚠ 语义逐字对齐旧版：旧版给 `times[0]`（atime）与 `times[1]`（mtime）**同一个值**，
/// 这里同样两个都设。
fn set_mtime(p: &Path, t: std::time::SystemTime) {
    let f = std::fs::File::options()
        .write(true)
        .open(p)
        .expect("打开要改 mtime 的那份文件失败，本条判据的前提没建起来");
    f.set_times(std::fs::FileTimes::new().set_accessed(t).set_modified(t))
        .expect("set_times 失败，本条判据的前提没建起来");
}

fn run_search(home: &Path, q: &str, limit: usize) -> Vec<Value> {
    let opts = SearchOpts {
        include_tools: false,
        scope: None,
        after_ms: 0,
        limit,
        titles: false,
    };
    let mut buf: Vec<u8> = Vec::new();
    search(home, q, &opts, &mut buf).expect("search ok");
    String::from_utf8(buf)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// `KR100D2`：**预算按最近优先花** —— 输出的行序就是预算顺序。
///
/// 死值验①（恢复成「按文件系统先走到的顺序」）当场红：语料刻意让
/// **创建序 / 名字序 与 mtime 序相反**，所以只要把 `sort_by_recency` 拿掉，
/// 第一行就不再是最新那个会话。
#[test]
fn the_snippet_budget_goes_to_the_most_recent_sessions() {
    // 创建序 old → mid → new；mtime 序 new(3000) > mid(2000) > old(1000)
    let home = corpus(
        "order",
        &[("old", 1_000, 3), ("mid", 2_000, 3), ("new", 3_000, 3)],
    );
    let rows = run_search(&home, "docker", 300);
    let ids: Vec<String> = rows
        .iter()
        .map(|r| r["sessionId"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["new", "mid", "old"],
        "预算/输出顺序必须是最近优先（与 monitor 的 `updated_at desc` 同一份 \
             `search_rules::sort_by_recency`）。收口前这里没有排序、按 readdir 走 —— \
             而两侧的**展示**顺序都是最近优先 ⇒ 缺 snippet 的正好是列表最上面那几张卡。"
    );

    // 预算只够 2 条 ⇒ 两条都必须花在**最新**那个会话上。
    let rows = run_search(&home, "docker", 2);
    let newest = rows.iter().find(|r| r["sessionId"] == "new").unwrap();
    assert_eq!(
        newest["hits"].as_array().unwrap().len(),
        2,
        "预算先给最新的"
    );
    for r in rows.iter().filter(|r| r["sessionId"] != "new") {
        assert_eq!(r["hits"].as_array().unwrap().len(), 0);
    }
    std::fs::remove_dir_all(&home).ok();
}

/// `KR100D3`：**截断说得出话**，而且与「本会话就这么点命中」分得开。
#[test]
fn truncation_is_stated_not_left_to_an_empty_array() {
    let home = corpus("trunc", &[("a", 3_000, 2), ("b", 2_000, 5)]);
    // 预算 2 ⇒ 全给 a，b 一条 snippet 都没有。
    let rows = run_search(&home, "docker", 2);
    let a = rows.iter().find(|r| r["sessionId"] == "a").unwrap();
    let b = rows.iter().find(|r| r["sessionId"] == "b").unwrap();
    assert_eq!(a["hitsTruncated"], false, "a 全给到了 ⇒ 没被砍");
    assert_eq!(b["hits"].as_array().unwrap().len(), 0);
    assert_eq!(b["hitCount"], 5);
    assert_eq!(
        b["hitsTruncated"], true,
        "🔴 `hitCount>0` 而 `hits: []` 必须自己说出「我被预算砍了」——\
             收口前这一格不存在，下游只能拿 `hitCount > hits.len()` 反推，\
             而那个式子对「预算砍的」与「本会话超 30 条」给出同一个答案。"
    );
    // 预算充足 ⇒ 两个都 false（反空真：这条判据不是恒 true）
    let rows = run_search(&home, "docker", 300);
    for r in &rows {
        assert_eq!(r["hitsTruncated"], false, "预算充足时不许乱报截断");
    }
    std::fs::remove_dir_all(&home).ok();
}

/// `KR100D1` 第 ③ 刀（本侧那一半）：**改 `search_rules` 一处，本侧真跑出来的东西跟着变**。
///
/// 期望值**从 `search_rules::SNIPPET_CTX` 取**，实际值从本文件的生产管线
/// （`search` → `session_hits_in` → `search_rules::make_snippet`）来。
/// · 改 core 的 `SNIPPET_CTX` ⇒ 实际与期望**一起动**，本条仍绿（＝行为跟着变了）；
/// · 本侧哪天自己写回一个 `const SNIPPET_CTX = 48` ⇒ 实际不动、期望动 ⇒ **当场红**。
/// monitor 侧原有一条同形的（随 monitor 内存索引一起删了：本机搜索也走本条测的这一份）。
#[test]
fn the_snippet_window_comes_from_core() {
    let ctx = search_rules::SNIPPET_CTX;
    let filler = "x".repeat(ctx * 4);
    let tmp = std::env::temp_dir().join(format!("ccm-kr100-ctx-{}", std::process::id()));
    std::fs::remove_dir_all(&tmp).ok();
    let proj = tmp.join("projects").join("p");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("s.jsonl"),
        format!(
            r#"{{"type":"user","uuid":"u","timestamp":"2026-01-01T00:00:00Z","cwd":"/w","message":{{"role":"user","content":"{filler}docker{filler}"}}}}"#
        ),
    )
    .unwrap();
    let rows = run_search(&tmp, "docker", 300);
    let hit = &rows[0]["hits"][0];
    // 两侧都截断了 ⇒ before = `…` + ctx 字符、after = ctx 字符 + `…`
    assert_eq!(
        hit["before"].as_str().unwrap().chars().count(),
        ctx + 1,
        "snippet 前窗必须等于 `search_rules::SNIPPET_CTX`（={ctx}）+ 省略号"
    );
    assert_eq!(hit["after"].as_str().unwrap().chars().count(), ctx + 1);
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔E 吞错普查点名 `search_query` 那一处〕**读不动的会话不许从结果里静默消失**：
/// 两份会话都含那个词，其中一份不是合法 UTF-8（整份读不动）⇒ 结果只有读得动的那一份（行形状不动），
/// 而这一趟回的「读不动」数 == 1、那句总账说出这个数；全都读得动 ⇒ 0、不说话（两向）。
///
/// （逐字）「处置不是别吞，是吞了要留一行日志」。
#[test]
fn w5vis_an_unreadable_session_is_counted_and_said_not_silently_dropped() {
    let tmp = std::env::temp_dir().join(format!("ccm-w5vis-search-{}", std::process::id()));
    std::fs::remove_dir_all(&tmp).ok();
    let proj = tmp.join("projects").join("p");
    std::fs::create_dir_all(&proj).unwrap();
    let rec = r#"{"type":"user","uuid":"u","timestamp":"2026-01-01T00:00:00Z","cwd":"/w","message":{"role":"user","content":"docker"}}"#;
    std::fs::write(proj.join("good.jsonl"), rec).unwrap();
    let opts = parse_opts(&[]);
    let mut buf = Vec::new();
    assert_eq!(
        search_counting(&tmp, "docker", &opts, &mut buf).expect("search ok"),
        0,
        "全都读得动却报了读不动"
    );
    let mut bad = rec.as_bytes().to_vec();
    bad.extend_from_slice(b"\n\xff\xfe docker\n");
    std::fs::write(proj.join("bad.jsonl"), &bad).unwrap();
    let mut buf = Vec::new();
    let n = search_counting(&tmp, "docker", &opts, &mut buf).expect("search ok");
    assert_eq!(n, 1, "读不动的那一份没被数到");
    let rows: Vec<Value> = String::from_utf8(buf)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 1, "结果行数不对：{rows:?}");
    assert_eq!(rows[0]["sessionId"], "good");
    assert_eq!(unreadable_note(0), None);
    let note = unreadable_note(n).expect("读不动 1 份却不说");
    assert!(note.contains('1'), "{note}");
    std::fs::remove_dir_all(&tmp).ok();
}

/// 要求：「多机合并排序收进 `search-core::sort_by_recency`，前端 `mergeSearchResults` 删」：
/// 界面照旧逐台扇出、合并排序问本机后端 `history-search-merge`。期望原样搬自 `tests/frontend/ui/views/history-search.vitest.ts` 那几条合并用例。
#[test]
fn the_merge_frame_sorts_newest_first_stably_and_sums_what_each_machine_said() {
    use serde_json::json;
    let mk = |sid: &str, updated: i64, hits: u64, cut: bool, origin: Option<&str>| {
        let mut v = json!({ "sessionId": sid, "updatedAt": updated, "hitCount": hits, "hitsTruncated": cut, "hits": [] });
        if let Some(o) = origin {
            v["origin"] = json!(o);
        }
        v
    };
    let merged = answer_merge(&json!({ "sessions": [
        mk("local-old", 100, 3, false, None),
        mk("rem-new", 300, 2, false, Some("pi")),
        mk("rem-mid", 200, 1, false, Some("wsl")),
        mk("tie-a", 150, 0, false, Some("pi")),
        mk("tie-b", 150, 0, false, None),
    ] }))
    .unwrap();
    let order: Vec<&str> = merged["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["sessionId"].as_str().unwrap())
        .collect();
    // 倒序；同一时刻保持交进来的先后（稳定）。
    assert_eq!(order, ["rem-new", "rem-mid", "tie-a", "tie-b", "local-old"]);
    assert_eq!(merged["totalHits"], 6);
    assert_eq!(merged["sessionCount"], 5);
    assert_eq!(merged["truncated"], false);
    // 其余各格原样透传（`origin` 缺 ＝ 本机，不补）。
    assert_eq!(merged["sessions"][0]["origin"], "pi");
    assert!(merged["sessions"][4].get("origin").is_none());
    // 任一台的任一会话被砍过 ⇒ 整体 truncated（`K-R100`）。
    for rows in [
        json!([
            mk("loc", 100, 1, false, None),
            mk("rem", 200, 12, true, Some("pi"))
        ]),
        json!([
            mk("loc", 100, 12, true, None),
            mk("rem", 200, 1, false, Some("pi"))
        ]),
    ] {
        assert_eq!(
            answer_merge(&json!({ "sessions": rows })).unwrap()["truncated"],
            true
        );
    }
    assert_eq!(
        answer_merge(&json!({ "sessions": [] })).unwrap(),
        json!({ "totalHits": 0, "sessionCount": 0, "truncated": false, "sessions": [] })
    );
    // 形状不对 ⇒ bad_args（不替界面补值）。
    for bad in [
        json!({}),
        json!({ "sessions": [ { "updatedAt": 1, "hitCount": 1 } ] }),
        json!({ "sessions": [ { "updatedAt": "1", "hitCount": 1, "hitsTruncated": false } ] }),
        json!({ "sessions": [ { "updatedAt": 1, "hitCount": -1, "hitsTruncated": false } ] }),
    ] {
        assert_eq!(answer_merge(&bad).unwrap_err().0, "bad_args");
    }
}

/// 命中带这台判的 `status` · `isBg` · `can`（与历史清单的行同一张规则，`history_query::can_of`）：
/// 在跑的（这台有它的 pidfile、进程活着）⇒ 切过去；后台分身 ⇒ 要恢复主会话、不能分叉；其余 ⇒ 能恢复。
/// 跨语言金样 `tests/__fixtures__/history-search-row.golden.json`（TS 严格解码器读同一份）。
#[test]
fn a_hit_carries_this_machines_status_and_can() {
    use serde_json::json;
    let home = corpus(
        "can",
        &[("live1", 3000, 1), ("ended1", 2000, 1), ("bg1", 1000, 1)],
    );
    let bg = home.join("projects/p-bg1/bg1.jsonl");
    let mut text = std::fs::read_to_string(&bg).unwrap();
    text.push_str("\n{\"type\":\"system\",\"sessionKind\":\"bg\"}");
    std::fs::write(&bg, text).unwrap();
    set_mtime(&bg, filetime_from_ms(1000));
    let sessions = crate::agents::claudecode::paths::sessions_root(&home);
    std::fs::create_dir_all(&sessions).unwrap();
    let pid = std::process::id();
    std::fs::write(
        sessions.join(format!("{pid}.json")),
        format!("{{\"pid\":{pid},\"sessionId\":\"live1\"}}"),
    )
    .unwrap();
    let rows = run_search(&home, "docker", 50);
    let prefix = home.canonicalize().unwrap().to_string_lossy().into_owned();
    std::fs::remove_dir_all(&home).ok();
    let pick = |r: &Value| json!([r["sessionId"], r["status"], r["isBg"], r["can"]]);
    assert_eq!(
        rows.iter().map(pick).collect::<Vec<_>>(),
        vec![
            json!(["live1", "live", false, {"resume": "switch", "accounts": true, "fork": true, "delete": "live"}]),
            json!(["ended1", "ended", false, {"resume": "yes", "accounts": true, "fork": true, "delete": "yes"}]),
            json!(["bg1", "ended", true, {"resume": "bg", "accounts": true, "fork": false, "delete": "yes"}]),
        ],
        "命中的状态与能做什么不是这台按 pidfile 与记录判的"
    );
    let got = json!({ "lines": rows
        .iter()
        .map(|r| r.to_string().replace(&prefix, "<HOME>"))
        .collect::<Vec<_>>() });
    let path =
        crate::guard_support::repo_root().join("tests/__fixtures__/history-search-row.golden.json");
    if std::env::var_os("CCM_BLESS").is_some() {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&got).unwrap()),
        )
        .unwrap();
    }
    let golden: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("金样")).unwrap();
    assert_eq!(
        got, golden,
        "命中行形状变了：TS 解码器读的是同一份金样，两边一起改（CCM_BLESS=1 重写）"
    );
}
