//! 设计/99 §2.1 ⑬ 主会话裁「`test_remote_connection`：界面把表单里（未保存的）那台配置交给本机后端，后端组拨号请求」·
//! 「端口转发：流没起的远端不许拒，本机后端按配置自己拨」—— `dial/machine.rs` 的规则判据（从 monitor `dial_host::request` /
//! `ssh_source::parse_address_line` 搬来：地址四形态 · 指纹只继承同一个 host 的 · 跳板查无 / 环都拒）。
use super::*;

fn m(v: Value) -> Machine {
    Machine::parse(&v).unwrap()
}

#[test]
fn address_lines_read_the_four_shapes_and_refuse_garbage() {
    assert_eq!(parse_address_line("h", 22), Some(("h".into(), 22)));
    assert_eq!(parse_address_line(" h:2222 ", 22), Some(("h".into(), 2222)));
    assert_eq!(
        parse_address_line("[::1]:2200", 22),
        Some(("::1".into(), 2200))
    );
    assert_eq!(
        parse_address_line("fe80::1", 22),
        Some(("fe80::1".into(), 22))
    );
    assert_eq!(parse_address_line(":22", 22), None, "只有端口没有 host");
    assert_eq!(
        parse_address_line("h:x", 22),
        None,
        "端口非法 ⇒ 拒，不静默默认"
    );
    assert_eq!(parse_address_line("  ", 22), None);
}

#[test]
fn the_request_carries_the_form_and_only_inherits_a_same_host_fingerprint() {
    let form = m(
        json!({"host": "10.0.0.2", "label": "aya", "port": 2222, "user": "u", "keyPath": "", "hostKeyFingerprint": "",
                        "addresses": ["aya.lan", "10.0.0.2:2222", "bad:x"], "jump": ""}),
    );
    let saved_same =
        m(json!({"host": "10.0.0.2", "user": "u", "hostKeyFingerprint": "SHA256:old"}));
    let saved_moved =
        m(json!({"host": "10.0.0.9", "user": "u", "hostKeyFingerprint": "SHA256:old"}));
    let r = request(
        &form,
        Some(&saved_same),
        None,
        "stream",
        json!({"stages": true}),
    )
    .unwrap();
    assert_eq!(
        r["host_key_fingerprint"], "SHA256:old",
        "表单空、已保存那份同一个 host ⇒ 继承"
    );
    assert_eq!(r["key_path"], Value::Null, "空串 ⇒ 走 agent");
    assert_eq!(
        r["endpoints"],
        json!([{"host": "10.0.0.2", "port": 2222}, {"host": "aya.lan", "port": 2222}]),
        "host 排首、去重、坏行丢"
    );
    assert_eq!(
        (r["use"].as_str(), r["stages"].as_bool()),
        (Some("stream"), Some(true))
    );
    let r = request(&form, Some(&saved_moved), None, "stream", json!({})).unwrap();
    assert_eq!(
        r["host_key_fingerprint"],
        Value::Null,
        "换了 host 不许继承旧指纹"
    );
    crate::dial::parse_request_value(&r).expect("组出来的请求后端自己读得动");
}

#[test]
fn a_jump_must_be_handed_over_and_must_not_point_at_itself() {
    let via = m(json!({"host": "10.0.0.2", "label": "aya", "user": "u", "jump": "bastion"}));
    let bastion =
        m(json!({"host": "b.example", "label": "bastion", "user": "j", "jump": "elsewhere"}));
    let r = request(&via, None, Some(&bastion), "forward", json!({})).unwrap();
    assert_eq!(r["jump"]["host"], "b.example");
    assert_eq!(r["jump"]["label"], "bastion");
    assert_eq!(
        request(&via, None, None, "forward", json!({}))
            .unwrap_err()
            .0,
        "bad_jump",
        "跳板没交来 ⇒ 拒（不回落直连）"
    );
    let wrong = m(json!({"host": "x", "label": "other", "user": "j"}));
    assert_eq!(
        request(&via, None, Some(&wrong), "forward", json!({}))
            .unwrap_err()
            .0,
        "bad_jump"
    );
    let selfish = m(json!({"host": "10.0.0.2", "label": "aya", "user": "u", "jump": "aya"}));
    assert_eq!(
        request(&selfish, None, Some(&selfish), "forward", json!({}))
            .unwrap_err()
            .0,
        "bad_jump",
        "环"
    );
}
