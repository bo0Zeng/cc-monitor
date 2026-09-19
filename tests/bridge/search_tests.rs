use super::*;

// ⚠ `K-R100` 的性能台架（同进程配对：`search_core::make_snippet` vs 收口前那份
// 逐字相同的本地副本）**跑完就删了**，读数落在 `tests/evidence/K-R100-deathvalue.md`。
// 它是一次性量具，不该留在门禁里（留下就成了一条没人跑、也没人维护的 `#[ignore]`）。

#[test]
fn query_scope_and_time_filter() {
    let idx = SearchIndex::new();
    {
        let mut d = idx.inner.write();
        let mk = |uuid: &str, ts: i64, kind: Kind, main: &str| MsgDoc {
            uuid: uuid.into(),
            ts_ms: ts,
            kind,
            main: main.into(),
            main_lc: main.to_lowercase(),
            tool: String::new(),
            tool_lc: String::new(),
        };
        d.sessions = vec![SessionDoc {
            session_id: "s1".into(),
            project_path: "/x".into(),
            project_name: "x".into(),
            jsonl_path: "/a.jsonl".into(),
            title: "t".into(),
            updated_at: 100,
            msgs: vec![
                mk("u1", 100, Kind::User, "deploy docker now"),
                mk("a1", 200, Kind::Assistant, "use docker compose"),
            ],
        }];
        d.total_messages = 2;
        d.ready = true;
    }
    // 全部：两条都命中
    assert_eq!(idx.query("docker", false, None, 0, 300).total_hits, 2);
    // 只 user：只 u1
    let user = idx.query("docker", false, Some(Kind::User), 0, 300);
    assert_eq!(user.total_hits, 1);
    assert_eq!(user.sessions[0].hits[0].uuid, "u1");
    // 只 assistant：只 a1
    let asst = idx.query("docker", false, Some(Kind::Assistant), 0, 300);
    assert_eq!(asst.total_hits, 1);
    assert_eq!(asst.sessions[0].hits[0].uuid, "a1");
    // 时间 >=150：只 a1（u1 的 ts=100 被滤掉）
    let recent = idx.query("docker", false, None, 150, 300);
    assert_eq!(recent.total_hits, 1);
    assert_eq!(recent.sessions[0].hits[0].uuid, "a1");
}

/// 契约测试：wire 全 camelCase，前端 TS interface 字段名须一致。
#[test]
fn search_response_camel_case_contract() {
    let resp = SearchResponse {
        status: "ready".into(),
        total_hits: 5,
        session_count: 1,
        truncated: false,
        indexed_sessions: 10,
        indexed_messages: 200,
        sessions: vec![SessionHits {
            session_id: "s1".into(),
            project_path: "/x".into(),
            project_name: "x".into(),
            jsonl_path: "/a.jsonl".into(),
            title: "t".into(),
            updated_at: 1,
            hit_count: 2,
            hits: vec![Hit {
                uuid: "u1".into(),
                ts_ms: 1,
                kind: "user".into(),
                before: "b".into(),
                matched: "m".into(),
                after: "a".into(),
            }],
            hits_truncated: true,
            origin: None,
        }],
    };
    let j = serde_json::to_string(&resp).unwrap();
    for k in [
        "\"totalHits\"",
        "\"sessionCount\"",
        "\"indexedSessions\"",
        "\"indexedMessages\"",
        "\"sessionId\"",
        "\"projectPath\"",
        "\"projectName\"",
        "\"jsonlPath\"",
        "\"updatedAt\"",
        "\"hitCount\"",
        "\"hitsTruncated\"",
        "\"tsMs\"",
    ] {
        assert!(j.contains(k), "wire 缺 {k}: {j}");
    }
    for snake in [
        "\"total_hits\"",
        "\"session_id\"",
        "\"jsonl_path\"",
        "\"ts_ms\"",
        "\"hits_truncated\"",
    ] {
        assert!(!j.contains(snake), "wire 漏改 {snake}: {j}");
    }
}

// === #28 远端搜索合并 ===

fn mk_session(sid: &str, updated: i64, hit_count: u32, origin: Option<&str>) -> SessionHits {
    SessionHits {
        session_id: sid.into(),
        project_path: "/p".into(),
        project_name: "p".into(),
        jsonl_path: format!("/{sid}.jsonl"),
        title: sid.into(),
        updated_at: updated,
        hit_count,
        hits: vec![],
        hits_truncated: false,
        origin: origin.map(str::to_string),
    }
}

fn resp(status: &str, total: u32, sessions: Vec<SessionHits>) -> SearchResponse {
    SearchResponse {
        status: status.into(),
        total_hits: total,
        session_count: sessions.len() as u32,
        truncated: false,
        indexed_sessions: 1,
        indexed_messages: 1,
        sessions,
    }
}

/// daemon 的 `--search` 输出（camelCase，无 origin）能反序列化成 SessionHits。
#[test]
fn session_hits_deserializes_from_daemon_json() {
    let line = r#"{"sessionId":"s9","projectPath":"/home/pi/p","projectName":"p","jsonlPath":"/home/pi/.claude/projects/p/s9.jsonl","title":"标题","updatedAt":123,"hitCount":2,"hits":[{"uuid":"u1","tsMs":5,"kind":"user","before":"b","matched":"m","after":"a"}]}"#;
    let sh: SessionHits = serde_json::from_str(line).expect("daemon json deserializes");
    assert_eq!(sh.session_id, "s9");
    assert_eq!(sh.hit_count, 2);
    assert_eq!(sh.hits.len(), 1);
    assert_eq!(
        sh.origin, None,
        "daemon 不发 origin → None（由 fan-out 补）"
    );
}

/// 合并：拼接 + updatedAt desc 重排 + 总数相加；远端 origin 保留。
#[test]
fn merge_orders_and_sums() {
    let local = resp("ready", 3, vec![mk_session("local-old", 100, 3, None)]);
    let remote = vec![
        mk_session("rem-new", 300, 2, Some("pi")),
        mk_session("rem-mid", 200, 1, Some("wsl")),
    ];
    let merged = merge_search_results(local, remote);
    assert_eq!(merged.status, "ready");
    assert_eq!(merged.total_hits, 3 + 2 + 1);
    assert_eq!(merged.session_count, 3);
    // updatedAt desc：rem-new(300) > rem-mid(200) > local-old(100)
    let ids: Vec<&str> = merged
        .sessions
        .iter()
        .map(|s| s.session_id.as_str())
        .collect();
    assert_eq!(ids, vec!["rem-new", "rem-mid", "local-old"]);
    assert_eq!(merged.sessions[0].origin.as_deref(), Some("pi"));
    assert_eq!(merged.sessions[2].origin, None);
}

/// 无远端 → 原样返回本地（含 indexing 态不被改写）。
#[test]
fn merge_no_remote_returns_local_verbatim() {
    let local = resp("indexing", 0, vec![]);
    let merged = merge_search_results(local, vec![]);
    assert_eq!(merged.status, "indexing");
    assert_eq!(merged.session_count, 0);
}

// ── `K-R100` 的行为判据（本侧那一半；daemon 侧有同形的三条）─────────────

/// 建一棵 `<claude_dir>/projects/<proj>/<sid>.jsonl`，mtime 按给定毫秒设。
fn corpus(tag: &str, sessions: &[(&str, u64, usize)]) -> PathBuf {
    let tmp = std::env::temp_dir().join(format!("ccm-kr100-mon-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&tmp).ok();
    for (sid, mtime_ms, n) in sessions {
        let proj = tmp.join("projects").join(format!("p-{sid}"));
        std::fs::create_dir_all(&proj).unwrap();
        let jsonl = proj.join(format!("{sid}.jsonl"));
        let lines: Vec<String> = (0..*n)
            .map(|i| {
                format!(
                    r#"{{"type":"user","uuid":"{sid}-{i}","timestamp":"2026-01-01T00:00:0{}Z","cwd":"/w","message":{{"role":"user","content":"命中 docker 第 {i} 条"}}}}"#,
                    i % 10
                )
            })
            .collect();
        std::fs::write(&jsonl, lines.join("\n")).unwrap();
        let f = File::options().write(true).open(&jsonl).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + Duration::from_millis(*mtime_ms))
            .unwrap();
    }
    tmp
}
fn built(dir: &Path) -> SearchIndex {
    let idx = SearchIndex::new();
    idx.build_blocking(dir, Duration::ZERO);
    idx
}

/// `KR100D2`：**预算按最近优先花**，与 daemon 同一份 `search_core::sort_by_recency`。
/// 死值验①（把某一侧换回「文件系统先走到的顺序」）当场红。
#[test]
fn the_snippet_budget_goes_to_the_most_recent_sessions() {
    let dir = corpus(
        "order",
        &[("old", 1_000, 3), ("mid", 2_000, 3), ("new", 3_000, 3)],
    );
    let idx = built(&dir);
    let r = idx.query("docker", false, None, 0, 300);
    let ids: Vec<&str> = r.sessions.iter().map(|s| s.session_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["new", "mid", "old"],
        "会话按最近优先排（= 预算顺序 = 展示顺序）"
    );

    // 预算只够 2 条 ⇒ 都花在最新那个会话上，与 daemon 侧同名判据逐条同形。
    let r = idx.query("docker", false, None, 0, 2);
    let newest = r.sessions.iter().find(|s| s.session_id == "new").unwrap();
    assert_eq!(newest.hits.len(), 2);
    for s in r.sessions.iter().filter(|s| s.session_id != "new") {
        assert!(s.hits.is_empty());
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// `KR100D3`（本地半）：截断自己说出来，且**不与「本会话就这么点命中」同形**。
#[test]
fn truncation_is_stated_not_left_to_an_empty_array() {
    let dir = corpus("trunc", &[("a", 3_000, 2), ("b", 2_000, 5)]);
    let idx = built(&dir);
    let r = idx.query("docker", false, None, 0, 2);
    assert!(r.truncated, "整份结果被全局预算砍过 ⇒ 状态行要说得出");
    let a = r.sessions.iter().find(|s| s.session_id == "a").unwrap();
    let b = r.sessions.iter().find(|s| s.session_id == "b").unwrap();
    assert!(!a.hits_truncated, "a 全给到了");
    assert_eq!((b.hit_count, b.hits.len()), (5, 0));
    assert!(
        b.hits_truncated,
        "🔴 `hitCount>0` 而 `hits: []` 必须自己说出「我被砍了」"
    );
    // 反空真：预算充足时一格都不许亮。
    let r = idx.query("docker", false, None, 0, 300);
    assert!(!r.truncated);
    assert!(r.sessions.iter().all(|s| !s.hits_truncated));
    std::fs::remove_dir_all(&dir).ok();
}

/// `KR100D3` 的合并半：**远端截断不许在合并那一步被丢掉**。
/// 收口前这里逐字 `truncated: local.truncated`。
#[test]
fn remote_truncation_survives_the_merge() {
    let local = resp("ready", 1, vec![mk_session("loc", 100, 1, None)]);
    assert!(!local.truncated);
    let mut rem = mk_session("rem", 200, 12, Some("pi"));
    rem.hits_truncated = true; // daemon 说的：它被自己的 --limit 砍了
    let merged = merge_search_results(local, vec![rem]);
    assert!(
        merged.truncated,
        "远端截断在合并处被丢掉了 —— 那正是「远端截断界面一个字不说」的成因"
    );
    // 反空真：远端没截断时不许乱亮。
    let local2 = resp("ready", 1, vec![mk_session("loc", 100, 1, None)]);
    let merged2 = merge_search_results(local2, vec![mk_session("rem", 200, 1, Some("pi"))]);
    assert!(!merged2.truncated);
}

/// `KR100D1` 第 ③ 刀（本侧那一半）：**改 `search_core` 一处，本侧真跑出来的东西跟着变**。
///
/// 期望值取自 `search_core::SNIPPET_CTX`，实际值来自本文件的生产管线
/// （`build_blocking` → `query` → `search_core::make_snippet`）。
/// · 改 core 的 `SNIPPET_CTX` ⇒ 两头一起动，本条仍绿（＝行为确实跟着变）；
/// · 本侧哪天写回一个自己的 `const SNIPPET_CTX = 48` ⇒ 实际不动、期望动 ⇒ **当场红**。
/// daemon 侧有一条同形的（`observe/search_query.rs::the_snippet_window_comes_from_core`）。
#[test]
fn the_snippet_window_comes_from_core() {
    let ctx = search_core::SNIPPET_CTX;
    let filler = "x".repeat(ctx * 4);
    let dir = std::env::temp_dir().join(format!("ccm-kr100-mon-ctx-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let proj = dir.join("projects").join("p");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("s.jsonl"),
        format!(
            r#"{{"type":"user","uuid":"u","timestamp":"2026-01-01T00:00:00Z","cwd":"/w","message":{{"role":"user","content":"{filler}docker{filler}"}}}}"#
        ),
    )
    .unwrap();
    let r = built(&dir).query("docker", false, None, 0, 300);
    let h = &r.sessions[0].hits[0];
    assert_eq!(
        h.before.chars().count(),
        ctx + 1,
        "snippet 前窗必须等于 `search_core::SNIPPET_CTX`（={ctx}）+ 省略号"
    );
    assert_eq!(h.after.chars().count(), ctx + 1);
    std::fs::remove_dir_all(&dir).ok();
}

/// 本地 indexing 但有远端结果 → status=ready（不丢远端）。
#[test]
fn merge_indexing_local_with_remote_is_ready() {
    let local = resp("indexing", 0, vec![]);
    let remote = vec![mk_session("rem", 50, 4, Some("pi"))];
    let merged = merge_search_results(local, remote);
    assert_eq!(merged.status, "ready");
    assert_eq!(merged.total_hits, 4);
    assert_eq!(merged.sessions.len(), 1);
}
