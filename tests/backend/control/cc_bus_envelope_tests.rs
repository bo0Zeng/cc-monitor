use super::*;
use serde_json::json;

const A: &str = "0f3c2a1e-1111-2222-3333-444455556666";
const B: &str = "9a8b7c6d-aaaa-bbbb-cccc-ddddeeeeffff";

/// 入参照线上那一份收（同分派走的 typed 那一处），再交本体。
fn check_json(v: serde_json::Value) -> Result<Checked, &'static str> {
    let req: crate::control::cc_bus_wire::BusEnvelopeReq =
        crate::wire::typed::take(&v).map_err(|_| "bad_args")?;
    check(&req).map_err(|(c, _)| c)
}

fn ok_req() -> serde_json::Value {
    json!({"from": A, "to": B, "act": "inform", "title": "占位标题", "text": "占位正文"})
}

fn with(k: &str, v: serde_json::Value) -> serde_json::Value {
    let mut r = ok_req();
    r[k] = v;
    r
}

fn code_of(v: serde_json::Value) -> &'static str {
    match check_json(v) {
        Ok(_) => "ok",
        Err(c) => c,
    }
}

#[test]
fn a_well_formed_envelope_passes_and_tier_defaults_to_two() {
    let c = check_json(ok_req()).expect("合格的信封要过");
    assert_eq!(c.tier, 2, "tier 缺省 2");
    assert_eq!(c.act, cmd_wire::bus::BusAct::Inform);
    assert!(c.refs.is_empty());
    assert_eq!(c.re, None);
}

#[test]
fn from_must_be_a_session_id() {
    assert_eq!(
        code_of(with("from", json!("planner"))),
        "ok",
        "正控：字母数字也是会话 id 的形状"
    );
    assert_eq!(
        code_of(with("from", json!("a_cc"))),
        "bad_from",
        "下划线不在会话 id 里（旧名字）"
    );
    assert_eq!(code_of(with("from", json!("../x"))), "bad_from");
    assert_eq!(code_of(with("from", json!(""))), "bad_from");
    assert_eq!(code_of(with("from", json!("cc-monitor x"))), "bad_from");
}

#[test]
fn to_must_be_a_session_id() {
    assert_eq!(code_of(with("to", json!("a/b"))), "bad_to");
    assert_eq!(code_of(with("to", json!("-x"))), "bad_to");
}

#[test]
fn tier_is_one_or_two() {
    assert_eq!(code_of(with("tier", json!(1))), "ok");
    assert_eq!(code_of(with("tier", json!(2))), "ok");
    assert_eq!(code_of(with("tier", json!(0))), "bad_tier");
    assert_eq!(code_of(with("tier", json!(3))), "bad_tier");
}

#[test]
fn act_is_one_of_the_six_codes() {
    for a in ["ask", "request", "inform"] {
        assert_eq!(code_of(with("act", json!(a))), "ok", "{a}");
    }
    for a in ["answer", "handback", "correct"] {
        let mut r = with("act", json!(a));
        r["re"] = json!("m18f2b3c4d5e6abcd");
        assert_eq!(code_of(r), "ok", "{a} 带 re");
    }
    // 是类型：写错的码 · 显示用的词 · 空串在解入参那一下就拒。
    assert_eq!(code_of(with("act", json!("notify"))), "bad_args");
    assert_eq!(code_of(with("act", json!("Ask"))), "bad_args");
    assert_eq!(code_of(with("act", json!(""))), "bad_args");
    let mut no_act = ok_req();
    no_act.as_object_mut().unwrap().remove("act");
    assert_eq!(code_of(no_act), "bad_args", "act 必填");
}

#[test]
fn title_is_required_one_line_at_most_eighty_chars() {
    let mut no_title = ok_req();
    no_title.as_object_mut().unwrap().remove("title");
    assert_eq!(code_of(no_title), "bad_args", "缺标题：解不出入参");
    assert_eq!(code_of(with("title", json!(""))), "bad_title", "空标题");
    assert_eq!(code_of(with("title", json!("   "))), "bad_title", "全空白");
    assert_eq!(
        code_of(with("title", json!("一行\n两行"))),
        "bad_title",
        "两行"
    );
    assert_eq!(code_of(with("title", json!("a\rb"))), "bad_title");
    assert_eq!(
        code_of(with("title", json!("字".repeat(80)))),
        "ok",
        "80 个字刚好"
    );
    assert_eq!(
        code_of(with("title", json!("字".repeat(81)))),
        "bad_title",
        "81 个字"
    );
}

#[test]
fn answer_handback_and_correction_must_carry_re() {
    for a in ["answer", "handback", "correct"] {
        assert_eq!(code_of(with("act", json!(a))), "need_re", "{a} 不带 re");
    }
    let mut r = with("act", json!("answer"));
    r["re"] = json!("not an id");
    assert_eq!(code_of(r), "bad_re", "re 要是消息 id 的形状");
    let mut r = with("act", json!("ask"));
    r["re"] = json!("m18f2b3c4d5e6abcd");
    assert_eq!(code_of(r), "ok", "问也可以接在一条后面");
}

#[test]
fn refs_only_check_the_prefix_shape() {
    let good = [
        "src/shared/cc-bus/SKILL.md",
        "/home/x/notes.md",
        "pb:A13-2/A13-2-1",
        "msg:m18f2b3c4d5e6abcd",
        "commit:393349b8b",
        "https://example.com/a?b=c",
        "http://localhost:8080/",
    ];
    assert_eq!(code_of(with("refs", json!(good))), "ok");
    for bad in [
        "",
        "pb:",
        "pb:A13",
        "pb:/x",
        "msg:nope",
        "commit:xyz",
        "commit:12",
        "https://",
        "ftp://x",
        "a b.md",
        "two\nlines",
    ] {
        assert_eq!(code_of(with("refs", json!([bad]))), "bad_ref", "{bad:?}");
    }
}

#[test]
fn body_is_at_most_1200_bytes() {
    assert_eq!(code_of(with("text", json!("a".repeat(1200)))), "ok");
    assert_eq!(code_of(with("text", json!("a".repeat(1201)))), "too_long");
    // 按字节算，不按字：400 个汉字 ＝ 1200 字节，401 个就超了。
    assert_eq!(code_of(with("text", json!("字".repeat(400)))), "ok");
    assert_eq!(code_of(with("text", json!("字".repeat(401)))), "too_long");
    let mut no_text = ok_req();
    no_text.as_object_mut().unwrap().remove("text");
    assert_eq!(code_of(no_text), "ok", "只有标题也行");
}

#[test]
fn every_refusal_code_is_in_the_command_code_table() {
    use crate::wire::typed::Codes;
    let table = crate::control::cc_bus_wire::BusSendErr::ALL;
    for c in [
        "bad_from",
        "bad_to",
        "bad_tier",
        "bad_title",
        "need_re",
        "bad_re",
        "bad_ref",
        "too_long",
    ] {
        assert!(table.contains(&c), "{c} 不在 bus-send 的码表里");
    }
}

#[test]
fn sealing_stamps_an_id_that_re_accepts_and_a_utc_time() {
    let c = check_json(ok_req()).unwrap();
    let e = seal(c, 1_761_465_600_123, 0xbeef);
    assert!(msg_id_ok(&e.id), "盖出来的 id 要过 re 那一道：{}", e.id);
    assert_eq!(e.ts, "2025-10-26T08:00:00Z");
    assert_eq!(e.tier, 2);
    assert_eq!(e.from, A);
    let again = seal(check_json(ok_req()).unwrap(), 1_761_465_600_123, 0xbeee);
    assert_ne!(e.id, again.id, "同一毫秒两条也不撞");
}

#[test]
fn sealed_envelope_round_trips_through_check() {
    // 盖过的那份照原样再交一遍也过得去（同一封重发：各格都合规矩）。
    let e = seal(check_json(ok_req()).unwrap(), 1_761_465_600_123, 7);
    let v = json!({"from": e.from, "to": e.to, "tier": e.tier, "act": e.act, "title": e.title, "refs": e.refs, "text": e.text});
    assert_eq!(code_of(v), "ok");
}
