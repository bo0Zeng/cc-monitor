//! 全文搜索那两条 typed 帧命令的判据。夹具只造结构（目录名 · 一两行占位 JSON），不采真会话正文。

use super::*;
use crate::wire::SessionFaceExt as _;
use cmd_wire::Machine;

/// 没有注解 · 不隐藏的那台说法。
fn plain(row: &cmd_wire::search::SessionHits) -> Marks {
    Marks {
        label: row.title.clone(),
        starred: false,
        hidden: None,
    }
}

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
        search_at(
            &home,
            req,
            crate::faces::read_face::LINES_CAP_BYTES,
            0,
            &Default::default(),
            &plain,
        )
        .expect("搜索")
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
        (
            v.sessions[0].row.title.as_str(),
            v.sessions[0].row.hits.len(),
            v.sessions[0].row.more_hits
        ),
        ("订单重试", 0, 0)
    );
    std::fs::remove_dir_all(&home).ok();
}

/// 每行带 `atText`（行尾那一格）与 `spanText`（打开它时内容头那一段）：按 `updatedAt`、按问的那一方的时区由这台写好（合几台时原样带过去）。
#[test]
fn each_row_carries_its_row_and_span_texts_written_here() {
    let home = std::env::temp_dir().join(format!("b8-search-texts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("-w-t");
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("0000aaaa-0000-4000-8000-0000000000c1.jsonl");
    std::fs::write(
        &f,
        r#"{"type":"user","uuid":"u1","timestamp":"2026-10-07T09:05:00Z","cwd":"/w/t","message":{"role":"user","content":"占位词"}}"#,
    )
    .unwrap();
    let day = crate::common::time::days_from_civil(2026, 10, 7) * 86_400;
    let at =
        std::time::UNIX_EPOCH + std::time::Duration::from_secs((day + 9 * 3_600 + 5 * 60) as u64);
    std::fs::File::options()
        .write(true)
        .open(&f)
        .unwrap()
        .set_modified(at)
        .unwrap();
    let req = HistorySearchReq {
        query: "占位词".to_string(),
        include_tools: None,
        scope: None,
        after_ms: None,
        limit: None,
        titles: None,
    };
    let now = (day + 15 * 3_600) * 1_000;
    let v = search_at(
        &home,
        req,
        crate::faces::read_face::LINES_CAP_BYTES,
        now,
        &Default::default(),
        &plain,
    )
    .expect("搜索");
    std::fs::remove_dir_all(&home).ok();
    let s = &v.sessions[0];
    assert_eq!(
        (s.at_text.as_str(), s.span_text.as_str()),
        ("09:05", "09:05–09:05")
    );
}

/// ★ 每行带那台此刻对这条会话的说法：显示的名字 · 星标 · 隐藏那一格（同历史清单那一行；界面不再拿清单去对）。
#[test]
fn each_row_carries_this_machines_label_star_and_hidden() {
    let home = std::env::temp_dir().join(format!("b8-search-marks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("-w-m");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("0000aaaa-0000-4000-8000-0000000000d1.jsonl"),
        r#"{"type":"user","uuid":"u1","timestamp":"2026-10-07T09:05:00Z","cwd":"/w/m","message":{"role":"user","content":"占位词"}}"#,
    )
    .unwrap();
    let req = HistorySearchReq {
        query: "占位词".to_string(),
        include_tools: None,
        scope: None,
        after_ms: None,
        limit: None,
        titles: None,
    };
    let mark = |row: &cmd_wire::search::SessionHits| Marks {
        label: format!("改过的-{}", &row.session_id[..4]),
        starred: true,
        hidden: Some(cmd_wire::session::Hidden {
            by: cmd_wire::session::HiddenBy::Own,
            said: "占位理由".into(),
        }),
    };
    let v = search_at(
        &home,
        req,
        crate::faces::read_face::LINES_CAP_BYTES,
        0,
        &Default::default(),
        &mark,
    )
    .expect("搜索");
    std::fs::remove_dir_all(&home).ok();
    let s = &v.sessions[0];
    assert_eq!(
        (
            s.label.as_str(),
            s.starred,
            s.hidden.as_ref().map(|h| h.said.as_str())
        ),
        ("改过的-0000", true, Some("占位理由"))
    );
}

// ───────── 几台合一份：本机后端自己问到各台、并好再答 ─────────

use crate::dial::remote_ask::test_peers::{until, Peers};
use serde_json::{json, Value};
use std::future::Future;

/// 一台的回包（真形：那台扫出来的行照金样，整数格都在），行改成给的会话 id 与时刻。
fn reply_text(rows: &[(&str, i64)]) -> String {
    let golden: Value = serde_json::from_str(include_str!(
        "../../__fixtures__/history-search-row.golden.json"
    ))
    .unwrap();
    let base: Value = serde_json::from_str(golden["lines"][0].as_str().unwrap()).unwrap();
    let sessions: Vec<Value> = rows
        .iter()
        .map(|(sid, at)| {
            let mut r = base.clone();
            r["sessionId"] = json!(sid);
            r["updatedAt"] = json!(at);
            r["atText"] = json!("09:05");
            r["spanText"] = json!("09:05–09:05");
            r["label"] = json!(format!("名-{sid}"));
            r["starred"] = json!(true);
            r["hidden"] = json!({"by": "self", "said": "已隐藏"});
            r
        })
        .collect();
    json!({"sessions": sessions, "unreadable": 1, "skipped": ["Codex"]}).to_string()
}

fn search_req(machines: &[&str]) -> HistorySearchMergeReq {
    HistorySearchMergeReq {
        machines: machines.iter().map(|m| Machine((*m).into())).collect(),
        search: HistorySearchReq {
            query: "docker".into(),
            include_tools: None,
            scope: None,
            after_ms: None,
            limit: Some(20),
            titles: None,
        },
    }
}

fn here(
    rows: &[(&str, i64)],
) -> impl Future<Output = Result<HistorySearchReply, HistorySearchErr>> + Send {
    let r: HistorySearchReply = serde_json::from_str(&reply_text(rows)).unwrap();
    std::future::ready(Ok(r))
}

/// ★ 本机后端自己问到各台（别的台经 `--resident-call` 问它的常驻 `history-search`，入参照发起方给的那一份、期限是发起方的），
/// 并好再答：几台的行按 `updatedAt` 倒序、远端的带那台的名字；那台常驻不在 ⇒ 那台照实进 `down`（说那台的名字）、别台照常；
/// 够不着（可达表里没有）同样进 `down`。整数格原样过。
#[tokio::test]
async fn the_local_backend_asks_each_machine_itself_and_merges() {
    let peers = Peers::new(&[("devbox", reply_text(&[("rem-new", 300), ("rem-old", 50)]))]);
    let t = Peers::table(&["devbox", "pi"]);
    let got = merge_with(
        search_req(&[cmd_wire::LOCAL, "devbox", "pi", "gone"]),
        here(&[("loc", 100)]),
        &Default::default(),
        until(9_000),
        &t,
        &peers,
    )
    .await
    .expect("合得出");
    let order: Vec<(&str, Option<&str>)> = got
        .sessions
        .iter()
        .map(|r| {
            (
                r.row.row.session_id.as_str(),
                r.origin.as_ref().map(|m| m.0.as_str()),
            )
        })
        .collect();
    assert_eq!(
        order,
        [
            ("rem-new", Some("devbox")),
            ("loc", None),
            ("rem-old", Some("devbox"))
        ]
    );
    assert_eq!(
        (got.unreadable, got.skipped.clone()),
        (2, vec!["Codex".to_string()])
    );
    let down: Vec<&str> = got.down.iter().map(|d| d.origin.0.as_str()).collect();
    assert_eq!(down, ["pi", "gone"], "没答上的那几台照实列出");
    assert!(
        got.down[0].said.contains("pi"),
        "常驻不在那一句说那台的名字：{}",
        got.down[0].said
    );
    let asked = peers.asked();
    assert_eq!(asked.len(), 2, "devbox · pi 各问一次，够不着的那台不拨");
    for (command, line) in asked.iter() {
        assert!(
            command.contains("--resident-call"),
            "问的不是那台的常驻：{command}"
        );
        let ms = Peers::within_ms(command);
        assert!(ms > 0 && ms <= 9_000, "交给那台的期限不是发起方的：{ms}");
        assert_eq!(line["cmd"], "history-search");
        assert_eq!(line["args"]["query"], "docker");
        assert_eq!(line["args"]["limit"], 20);
    }
    let out = serde_json::to_value(&got).unwrap();
    assert!(
        out["sessions"][0]["updatedAt"].is_i64() && out["sessions"][0]["face"]["order"].is_u64(),
        "{out}"
    );
}

/// 问到别的台却没带期限 ⇒ `bad_args`（等多久只归发起方）；只问本机不要期限。本机那一台没答上 ⇒ 整次照它的码失败。
#[tokio::test]
async fn asking_other_machines_needs_the_askers_deadline_and_the_local_one_must_answer() {
    let peers = Peers::new(&[]);
    let t = Peers::table(&["devbox"]);
    let no = merge_with(
        search_req(&[cmd_wire::LOCAL, "devbox"]),
        here(&[]),
        &Default::default(),
        None,
        &t,
        &peers,
    )
    .await;
    assert!(
        matches!(no, Err(HistorySearchMergeErr::BadArgs(_))),
        "{no:?}"
    );
    assert!(peers.asked().is_empty());
    let only_here = merge_with(
        search_req(&[cmd_wire::LOCAL]),
        here(&[("loc", 1)]),
        &Default::default(),
        None,
        &t,
        &peers,
    )
    .await;
    assert_eq!(only_here.expect("只问本机").session_count, 1);
    let broke = merge_with(
        search_req(&[cmd_wire::LOCAL]),
        std::future::ready(Err(HistorySearchErr::Failed("读不了".into()))),
        &Default::default(),
        None,
        &t,
        &peers,
    )
    .await;
    assert!(
        matches!(broke, Err(HistorySearchMergeErr::Failed(_))),
        "{broke:?}"
    );
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
    let small = search_at(&home, req(), 1024, 0, &Default::default(), &plain);
    let full = search_at(
        &home,
        req(),
        crate::faces::read_face::LINES_CAP_BYTES,
        0,
        &Default::default(),
        &plain,
    );
    std::fs::remove_dir_all(&home).ok();
    assert!(
        matches!(small, Err(HistorySearchErr::TooLarge(_))),
        "超了上限还交了回包：{small:?}"
    );
    assert_eq!(full.expect("生产上限下该照常答").sessions.len(), 4);
}

/// ★ 每条命中带它那一行在记录文件里的起点（`history-read` 从它读起就是那一条，不必从头读）；分叉来的会话带父会话的显示标题
/// （父会话没命中也认得出），父会话这一趟没走到 ⇒ 缺。
#[test]
fn hits_carry_their_offset_and_forks_name_their_parent() {
    let home = std::env::temp_dir().join(format!("b8-search-fork-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let dir = home.join("projects").join("-w-x");
    std::fs::create_dir_all(&dir).unwrap();
    const P: &str = "0000aaaa-0000-4000-8000-0000000000b1";
    const C: &str = "0000aaaa-0000-4000-8000-0000000000b2";
    const O: &str = "0000aaaa-0000-4000-8000-0000000000b3";
    let line = |uuid: &str, fork: Option<&str>, said: &str| {
        let fork = fork.map_or(String::new(), |f| {
            format!(r#","forkedFrom":{{"sessionId":"{f}","messageUuid":"m"}}"#)
        });
        format!(
            r#"{{"type":"user","uuid":"{uuid}","timestamp":"2026-01-01T00:00:00Z","cwd":"/w/x"{fork},"message":{{"role":"user","content":"{said}"}}}}"#
        ) + "\n"
    };
    std::fs::write(
        dir.join(format!("{P}.jsonl")),
        line("p1", None, "甲") + r#"{"type":"ai-title","aiTitle":"父标题"}"# + "\n",
    )
    .unwrap();
    let first = line("c1", Some(P), "乙");
    std::fs::write(
        dir.join(format!("{C}.jsonl")),
        first.clone() + "\r\n" + line("c2", Some(P), "命中词").as_str(),
    )
    .unwrap();
    std::fs::write(
        dir.join(format!("{O}.jsonl")),
        line("o1", Some("0000aaaa-0000-4000-8000-0000000000ff"), "命中词"),
    )
    .unwrap();
    let req = HistorySearchReq {
        query: "命中词".to_string(),
        include_tools: None,
        scope: None,
        after_ms: None,
        limit: None,
        titles: None,
    };
    let v = search_at(
        &home,
        req,
        crate::faces::read_face::LINES_CAP_BYTES,
        0,
        &Default::default(),
        &plain,
    )
    .expect("搜索");
    let by = |sid: &str| {
        v.sessions
            .iter()
            .find(|s| s.row.session_id == sid)
            .map(|s| &s.row)
            .unwrap_or_else(|| panic!("{sid} 没命中"))
    };
    let c = by(C);
    assert_eq!(
        c.forked_from.as_ref().map(|f| f.label.0.as_str()),
        Some("父标题")
    );
    assert_eq!(by(O).forked_from, None, "父会话不在 ⇒ 缺");
    let off = c.hits[0].offset;
    assert_eq!(off, (first.len() + 2) as u64);
    let bytes = std::fs::read(dir.join(format!("{C}.jsonl"))).unwrap();
    assert!(
        bytes[off as usize..].starts_with(br#"{"type":"user","uuid":"c2""#),
        "从 offset 读起不是那一条"
    );
    let wire = serde_json::to_value(c).unwrap();
    assert!(wire.get("parent").is_none(), "父会话 id 上了线");
    std::fs::remove_dir_all(&home).ok();
}
