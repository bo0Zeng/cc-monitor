//! 全文搜索那两条 typed 帧命令的判据。夹具只造结构（目录名 · 一两行占位 JSON），不采真会话正文。

use super::*;
use cmd_wire::Machine;

/// 全文搜索的应答把「读不动几份」与「内容搜索不覆盖的那几家」一起交回（此前只进日志，界面只能说「共 N 条」像是全的）；
/// `titles: true` 只比标题与第一句：只在标题里出现的词也搜得到那一个会话。
#[test]
fn the_search_answer_says_what_it_could_not_search_and_titles_mode_finds_title_only_words() {
    let home = std::env::temp_dir().join(format!("b8-search-face-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("-w-x");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("0000aaaa-0000-4000-8000-0000000000aa.jsonl"),
        concat!(
            r#"{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","cwd":"/w/x","message":{"role":"user","content":"占位正文"}}"#,
            "\n",
            r#"{"type":"ai-title","aiTitle":"订单重试"}"#,
            "\n"
        ),
    )
    .unwrap();
    let ask = |titles: bool| {
        let req = HistorySearchReq {
            query: "订单".to_string(),
            include_tools: None,
            scope: None,
            after_ms: None,
            limit: None,
            titles: Some(titles),
        };
        search_at(&home, req, crate::faces::read_face::LINES_CAP_BYTES).expect("搜索")
    };
    let v = ask(false);
    assert_eq!(v.unreadable, 0, "读不动几份没交回");
    assert!(
        v.sessions.is_empty(),
        "只在标题里的词被当成了内容命中：{:?}",
        v.sessions
    );
    let v = ask(true);
    assert_eq!(v.sessions.len(), 1, "按标题搜没搜到那一个会话");
    assert_eq!(
        (v.sessions[0].title.as_str(), v.sessions[0].hit_count),
        ("订单重试", 0)
    );
    std::fs::remove_dir_all(&home).ok();
}

/// 合并按台收：本机那一台的行不带名字，远端的带上；`updatedAt` 倒序；命中数相加、任一行被砍 ⇒ `truncated`。
#[test]
fn merge_takes_rows_per_machine_and_names_only_the_remote_ones() {
    let row = |sid: &str, at: i64, hits: u32, cut: bool| SessionHits {
        agent: "claude-code".into(),
        session_id: sid.into(),
        project_path: "/w".into(),
        project_name: "w".into(),
        jsonl_path: format!("/w/{sid}.jsonl"),
        title: sid.into(),
        updated_at: at,
        hit_count: hits,
        hits: vec![],
        hits_truncated: cut,
        is_bg: false,
        status: crate::observe::search_query::Liveness::Ended,
        can: crate::observe::history_query::can_of("claude-code", "ended", false),
    };
    let got = crate::observe::search_query::merge_at(
        vec![
            MachineHits {
                origin: Machine::local(),
                sessions: vec![row("a", 10, 2, false)],
            },
            MachineHits {
                origin: Machine("devbox".into()),
                sessions: vec![row("b", 20, 3, true)],
            },
        ],
        30,
        &Default::default(),
    );
    assert_eq!(
        (got.total_hits, got.session_count, got.truncated),
        (5, 2, true)
    );
    let order: Vec<(&str, Option<&str>)> = got
        .sessions
        .iter()
        .map(|r| {
            (
                r.row.session_id.as_str(),
                r.origin.as_ref().map(|m| m.0.as_str()),
            )
        })
        .collect();
    assert_eq!(order, vec![("b", Some("devbox")), ("a", None)]);
}

/// 回包过了上限 ⇒ `too_large`，不交截断的清单；正控：同一份夹具、生产那个上限下照常答。上限是 `search_at` 的入参，生产只传 `LINES_CAP_BYTES`。
#[test]
fn an_oversized_search_answer_is_refused_not_truncated() {
    let home = std::env::temp_dir().join(format!("b8-search-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("-w-y");
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..4 {
        std::fs::write(
            dir.join(format!("0000aaaa-0000-4000-8000-00000000000{i}.jsonl")),
            r#"{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","cwd":"/w/y","message":{"role":"user","content":"占位词"}}"#,
        )
        .unwrap();
    }
    let req = || HistorySearchReq {
        query: "占位词".to_string(),
        include_tools: None,
        scope: None,
        after_ms: None,
        limit: None,
        titles: None,
    };
    let small = search_at(&home, req(), 1024);
    let full = search_at(&home, req(), crate::faces::read_face::LINES_CAP_BYTES);
    std::fs::remove_dir_all(&home).ok();
    assert!(
        matches!(small, Err(HistorySearchErr::TooLarge(_))),
        "超了上限还交了回包：{small:?}"
    );
    assert_eq!(full.expect("生产上限下该照常答").sessions.len(), 4);
}
