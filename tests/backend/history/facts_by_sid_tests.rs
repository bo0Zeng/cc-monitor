//! `history-facts` 按会话 id 问（`{sid}`）：那台在自己的家目录里找那份记录、回包带找到的 `path`；
//! 事实与按路径问同一份；不认得的 sid ⇒ `no_such_session`；两样都交 · 都不交 ⇒ 入参不对。
use super::*;
use std::path::PathBuf;

fn scratch(tag: &str) -> PathBuf {
    let d = test_home::scratch(tag);
    std::fs::create_dir_all(d.join("projects")).unwrap();
    d
}

/// 一份最小的会话记录：开头带项目目录，一条带用量的回复。
fn record(home: &Path, sid: &str) -> PathBuf {
    let dir = home.join("projects").join("-w-proj");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("{sid}.jsonl"));
    let body = [
        r#"{"type":"user","uuid":"u-1","timestamp":"t0","cwd":"/w/proj","message":{"content":"q"}}"#,
        r#"{"type":"assistant","uuid":"a-1","parentUuid":"u-1","timestamp":"t1","message":{"model":"m-x","usage":{"input_tokens":1},"content":[{"type":"text","text":"done"}]}}"#,
    ]
    .map(|l| format!("{l}\n"))
    .concat();
    std::fs::write(&p, body).unwrap();
    p
}

fn req(v: serde_json::Value) -> Result<HistoryFactsReq, serde_json::Error> {
    serde_json::from_value(v)
}

fn ask(home: &Path, v: serde_json::Value) -> Result<serde_json::Value, HistoryFactsErr> {
    let limits = crate::observe::recognize::facts::ContextLimits::default();
    facts_at(
        home,
        req(v).expect("入参该读得进"),
        &limits,
        &crate::common::time::Tz::default(),
        None,
    )
    .map(|r| serde_json::to_value(r).unwrap())
}

#[test]
fn by_sid_finds_the_record_and_hands_back_its_path() {
    let home = scratch("found");
    let sid = "0b1c2d3e-aaaa-4bbb-8ccc-1234567890ab";
    let p = record(&home, sid);
    let by_sid = ask(&home, serde_json::json!({ "sid": sid })).unwrap();
    assert_eq!(
        by_sid["path"],
        serde_json::json!(p.to_string_lossy()),
        "回包带找到的那份记录：{by_sid}"
    );
    assert_eq!(by_sid["projectDir"], serde_json::json!("/w/proj"));
    let mut by_path = ask(&home, serde_json::json!({ "path": p.to_string_lossy() })).unwrap();
    assert!(
        by_path.get("path").is_none(),
        "按路径问的不带 path（就是交来的那一份）：{by_path}"
    );
    by_path["path"] = by_sid["path"].clone();
    assert_eq!(by_sid, by_path, "两种问法答的是同一份事实");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_unknown_sid_is_no_such_session_with_the_machine_sentence() {
    let home = scratch("missing");
    record(&home, "0b1c2d3e-aaaa-4bbb-8ccc-1234567890ab");
    match ask(
        &home,
        serde_json::json!({ "sid": "ffffffff-0000-4000-8000-000000000000" }),
    ) {
        Err(HistoryFactsErr::NoSuchSession(said)) => {
            assert_eq!(
                said,
                copy_core::copy_text("beSessionCommands.session.noRecord", &[])
            );
        }
        other => panic!("不认得的 sid 该是 no_such_session：{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn path_and_sid_are_one_or_the_other() {
    assert!(
        req(serde_json::json!({ "path": "/p/a.jsonl", "sid": "s1" })).is_err(),
        "两样都交 ⇒ 不认"
    );
    assert!(req(serde_json::json!({})).is_err(), "都不交 ⇒ 不认");
    assert!(
        req(serde_json::json!({ "sid": "s1", "prior": {} })).is_err(),
        "多一格 ⇒ 不认"
    );
}
