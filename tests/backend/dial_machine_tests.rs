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
        json!({"host": "10.0.0.2", "label": "devbox", "port": 2222, "user": "u", "keyPath": "", "hostKeyFingerprint": "",
                        "addresses": ["devbox.lan", "10.0.0.2:2222", "bad:x"], "jump": ""}),
    );
    let saved_same =
        m(json!({"host": "10.0.0.2", "user": "u", "hostKeyFingerprint": "SHA256:old"}));
    let saved_moved =
        m(json!({"host": "10.0.0.9", "user": "u", "hostKeyFingerprint": "SHA256:old"}));
    let r = request(
        &form,
        Some(&saved_same),
        None,
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
        json!([{"host": "10.0.0.2", "port": 2222}, {"host": "devbox.lan", "port": 2222}]),
        "host 排首、去重、坏行丢"
    );
    assert_eq!(
        (r["use"].as_str(), r["stages"].as_bool()),
        (Some("stream"), Some(true))
    );
    let r = request(&form, Some(&saved_moved), None, None, "stream", json!({})).unwrap();
    assert_eq!(
        r["host_key_fingerprint"],
        Value::Null,
        "换了 host 不许继承旧指纹"
    );
    serde_json::from_value::<crate::dial::DialRequest>(r).expect("组出来的请求后端自己读得动");
}

#[test]
fn a_jump_must_be_handed_over_and_must_not_point_at_itself() {
    let via = m(json!({"host": "10.0.0.2", "label": "devbox", "user": "u", "jump": "bastion"}));
    let bastion =
        m(json!({"host": "b.example", "label": "bastion", "user": "j", "jump": "elsewhere"}));
    let r = request(&via, None, Some(&bastion), None, "forward", json!({})).unwrap();
    assert_eq!(r["jump"]["host"], "b.example");
    assert_eq!(r["jump"]["label"], "bastion");
    assert_eq!(
        request(&via, None, None, None, "forward", json!({}))
            .unwrap_err()
            .0,
        "bad_jump",
        "跳板没交来 ⇒ 拒（不回落直连）"
    );
    let wrong = m(json!({"host": "x", "label": "other", "user": "j"}));
    assert_eq!(
        request(&via, None, Some(&wrong), None, "forward", json!({}))
            .unwrap_err()
            .0,
        "bad_jump"
    );
    let selfish = m(json!({"host": "10.0.0.2", "label": "devbox", "user": "u", "jump": "devbox"}));
    assert_eq!(
        request(&selfish, None, Some(&selfish), None, "forward", json!({}))
            .unwrap_err()
            .0,
        "bad_jump",
        "环"
    );
}

#[test]
fn a_wire_dial_is_composed_here_and_the_preferred_winner_goes_first() {
    // 〔MIG-1 收尾〕线上交来的是一台原样的配置（monitor 不再解析地址 / 组请求）：组法只在这里。
    let wire = json!({
        "machine": {"host": "10.0.0.2", "user": "u", "addresses": ["devbox.lan", "[fe80::1]:2200"]},
        "prefer": {"host": "fe80::1", "port": 2200},
        "use": "capture",
        "command": "x",
        "agent_sock": "/run/agent",
    });
    let r = resolve(&wire).unwrap();
    let order: Vec<(String, u16)> = r
        .race_order()
        .into_iter()
        .map(|e| (e.host, e.port))
        .collect();
    assert_eq!(
        order,
        vec![
            ("fe80::1".into(), 2200),
            ("10.0.0.2".into(), 22),
            ("devbox.lan".into(), 22)
        ],
        "上次赢的那条排首，其余保序"
    );
    assert_eq!(r.use_, crate::dial::Use::Capture);
    assert_eq!(
        (r.command.as_str(), r.agent_sock.as_deref()),
        ("x", Some("/run/agent"))
    );
    // 上次赢的那条已不在这台的地址里（配置改过）⇒ 顺序不动。
    let mut stale = wire.clone();
    stale["prefer"] = json!({"host": "gone.lan", "port": 22});
    let order: Vec<String> = resolve(&stale)
        .unwrap()
        .race_order()
        .into_iter()
        .map(|e| e.host)
        .collect();
    assert_eq!(order, vec!["10.0.0.2", "devbox.lan", "fe80::1"]);
    // 没有 `machine` ⇒ 拒（线上不再收组好的那一形）。
    assert_eq!(
        resolve(&json!({"host": "10.0.0.2", "user": "u", "port": 22}))
            .map(|_| ())
            .map_err(|(c, _)| c),
        Err("invalid_args")
    );
}

#[test]
fn the_ack_says_how_strict_the_composed_request_was() {
    // 〔MIG-1 收尾〕界面判「要不要自动固化」看 ack 的 `strict` / `jump_strict`，不再自己重推指纹继承（规则只在本文件）。
    let strict_of = |wire: Value| crate::dial::uses::strictness(&resolve(&wire).unwrap());
    let form = json!({"host": "10.0.0.2", "label": "devbox", "user": "u", "jump": "bastion"});
    let bastion = |fp: &str| json!({"host": "b.lan", "label": "bastion", "user": "u", "hostKeyFingerprint": fp});
    assert_eq!(
        strict_of(json!({"machine": form, "jump": bastion("")})),
        (false, false),
        "谁都没带指纹 ⇒ 两台都 TOFU"
    );
    assert_eq!(
        strict_of(
            json!({"machine": form, "saved": {"host": "10.0.0.2", "user": "u", "hostKeyFingerprint": "SHA256:s"},
            "jump": bastion("SHA256:j")})
        ),
        (true, true),
        "目标从同一个 host 的已保存那份继承了指纹 ⇒ 严格；跳板自己带了 ⇒ 严格"
    );
    assert_eq!(
        strict_of(
            json!({"machine": form, "saved": {"host": "10.9.9.9", "user": "u", "hostKeyFingerprint": "SHA256:s"},
            "jump": bastion("  ")})
        ),
        (false, false),
        "换了 host 不继承；空白指纹不算"
    );
}
