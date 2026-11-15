//! `session-rewind-options`：入参形状 · 交给那台时换成本机 · 这台没有那条会话的记录 ⇒ `no_such_session`。

use super::*;

#[test]
fn the_request_takes_exactly_machine_sid_and_the_line() {
    let ok: SessionRewindOptionsReq = serde_json::from_value(serde_json::json!({
        "machine": "<local>",
        "sid": "00000000-0000-4000-8000-0000000000a1",
        "to": "u2",
    }))
    .expect("三格就够");
    assert!(ok.machine.is_local());
    assert_eq!(ok.to, "u2");
    let no_line = serde_json::from_value::<SessionRewindOptionsReq>(serde_json::json!({
        "machine": "<local>",
        "sid": "00000000-0000-4000-8000-0000000000a1",
    }));
    assert!(
        no_line.is_err(),
        "不说是哪一句 ⇒ 拒（能不能退代码按那一句判）"
    );
    let extra = serde_json::from_value::<SessionRewindOptionsReq>(serde_json::json!({
        "machine": "<local>",
        "sid": "00000000-0000-4000-8000-0000000000a1",
        "to": "u2",
        "how": "conversation",
    }));
    assert!(extra.is_err(), "多一格 ⇒ 拒（选项只问，不带选了什么）");
}

#[test]
fn handing_to_the_other_machine_rewrites_it_as_local() {
    let req = SessionRewindOptionsReq {
        machine: Machine::named("box2"),
        sid: "00000000-0000-4000-8000-0000000000a1".parse().unwrap(),
        to: "u2".into(),
    };
    assert!(!req.machine().is_local());
    let there = req.here();
    assert!(there.machine.is_local());
    assert_eq!(there.sid, req.sid);
    assert_eq!(there.to, "u2");
}

/// 测试沙箱的家里没有这条会话 ⇒ `no_such_session`（不照缺省那一家瞎给选项）。
#[test]
fn a_session_without_a_record_here_is_no_such_session() {
    let got =
        options_here("00000000-0000-4000-8000-0000000000a9", "u1").expect_err("沙箱里没有这条会话");
    assert_eq!(got.code.as_deref(), Some("no_such_session"));
}

fn record(tag: &str, lines: &[&str]) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-rewind-after-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("sandbox");
    let p = d.join("s.jsonl");
    std::fs::write(&p, lines.join("\n") + "\n").expect("写夹具");
    p
}

/// 那一句之后的事从这台的记录 ＋ 运行表读：第一句之后改过代码、第二句之后没改；第二句之后派出、还在跑的数进来；记录里没有那一句 ⇒ `None`。
#[test]
fn what_happened_after_the_line_is_read_from_the_record_and_the_run_table() {
    // 第二句之后那一段比前面长：从文件头读同样多的字节会读进第一句之后那次改动（读错起点就红）。
    let long = format!(
        r#"{{"type":"assistant","uuid":"a3","parentUuid":"a2","timestamp":"2026-10-01T00:01:02.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"{}"}}]}}}}"#,
        "说".repeat(400)
    );
    let p = record(
        "after",
        &[
            r#"{"type":"user","uuid":"u1","timestamp":"2026-10-01T00:00:00.000Z","message":{"role":"user","content":"第一句"}}"#,
            r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-10-01T00:00:01.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/w/a.rs","old_string":"x","new_string":"y"}}]}}"#,
            r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-10-01T00:01:00.000Z","message":{"role":"user","content":"第二句"}}"#,
            r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","timestamp":"2026-10-01T00:01:01.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"/w/a.rs"}}]}}"#,
            &long,
        ],
    );
    let at = |iso: &str| crate::common::time::parse_iso8601_ms(iso).map(|t| t as u64);
    let runs = [
        cmd_wire::frame::RunInfo {
            started_ms: at("2026-10-01T00:00:30.000Z"),
            ..Default::default()
        },
        cmd_wire::frame::RunInfo {
            started_ms: at("2026-10-01T00:01:30.000Z"),
            ..Default::default()
        },
    ];
    use crate::control::rewind::After;
    // 夹具记录是默认那一家的形状（认家由调用方照按 sid 找到它的那一家交进来）。
    let kind = crate::agents::default_kind();
    assert_eq!(
        after_in(&p, kind, "u1", &runs).expect("读得到"),
        Some(After {
            code: true,
            runs: 2
        })
    );
    assert_eq!(
        after_in(&p, kind, "u2", &runs).expect("读得到"),
        Some(After {
            code: false,
            runs: 1
        })
    );
    assert_eq!(after_in(&p, kind, "zz", &runs).expect("读得到"), None);
}

/// 这台没有那条会话的记录 ⇒ 一个键不按，回不支持（测试沙箱的家是空的）。
#[test]
fn a_session_without_a_record_here_is_not_walked() {
    let got = walk_here(
        "00000000-0000-4000-8000-0000000000b1",
        &Rewind {
            to: "u1".into(),
            how: cmd_wire::rewind::RewindHow::Conversation,
            when: cmd_wire::rewind::RewindWhen::AfterTurn,
        },
        "x",
        None,
    )
    .expect("没有记录不是命令级错");
    assert_eq!(got, Walked::No(Why::Unsupported));
}

/// ★ 回退菜单照按 sid 找到它的那一家认：那一家没申报 ⇒ 没有（不支持、一个键不按），不落到默认那一家的菜单；申报了 ⇒ 就是它那一份。
#[test]
fn the_menu_follows_the_family_the_session_was_found_in() {
    let (reg, at) = found_in_the_other("rewind-none", None);
    assert!(
        face_among(&reg, &at).is_none(),
        "那一家没申报回退 ⇒ 没有菜单，不照默认那一家按键"
    );
    let (reg, at) = found_in_the_other("rewind-own", crate::agents::fake::LAUNCH.rewind);
    assert!(face_among(&reg, &at).is_some(), "申报了 ⇒ 照那一家的菜单");
}

/// ★ 弹选项判忙只照那一屏：空着的输入行（只有后台子 agent 在跑、主 agent 空着时活会话表照样说在跑，实测屏上就是这一屏）⇒ 不忙、
/// 不给「什么时候退」；屏上在出字 ⇒ 给，缺省等这一轮完；读不了屏 ⇒ 回退那一串退不了：只写为什么、一项不给（不拿「不忙」顶上）。
#[test]
fn the_options_judge_a_running_turn_by_the_screen_alone() {
    use crate::agents::fake::rewind::{FakeTerm, FACE};
    let mut t = FakeTerm::with(&["一"]);
    assert!(!turn_running(&FACE, &t.drawn()), "空着的输入行 ⇒ 不忙");
    let idle = options_on(&FACE, Ok(t.drawn()), After::default());
    assert!(!idle.how.is_empty());
    assert!(idle.when.is_empty(), "不忙 ⇒ 不给「什么时候退」");
    t.busy = true;
    assert!(turn_running(&FACE, &t.drawn()), "屏上在出字 ⇒ 忙");
    let busy = options_on(&FACE, Ok(t.drawn()), After::default());
    assert_eq!(
        busy.when.iter().find(|r| r.default).map(|r| r.when),
        Some(cmd_wire::rewind::RewindWhen::AfterTurn)
    );
    let gone = options_on(&FACE, Err("终端已结束".into()), After::default());
    assert!(
        gone.how.is_empty() && gone.when.is_empty(),
        "读不了屏 ⇒ 一项不给"
    );
    assert_eq!(gone.said.as_deref(), Some("终端已结束"), "照实写为什么");
}

/// 会话落在 `other` 那一家（照路径认不出），`other` 回退那一格照 `rewind`；默认那一家申报了回退菜单（[`crate::agents::fake::found_in_the_other`]）。
fn found_in_the_other(
    tag: &str,
    rewind: Option<RewindFace>,
) -> (Vec<crate::agents::Adapter>, crate::agents::SessionAt) {
    assert!(
        crate::agents::fake::LAUNCH.rewind.is_some(),
        "默认那一家申报了回退菜单"
    );
    crate::agents::fake::found_in_the_other(tag, |f| crate::agents::LaunchFace { rewind, ..f })
}
