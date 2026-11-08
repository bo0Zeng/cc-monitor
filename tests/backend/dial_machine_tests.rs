//! `ssh-dial/machine.rs` 的规则判据：一台机器（`Host`）的名字 · 地址四形态 · 判准 · 组拨号请求 · 跳板一路照表找（多跳；查无 / 环都拒）。
use super::*;

/// 线上那一形（键可缺）照 `Host` 的派生读成的一台。
fn m(v: Value) -> Host {
    <Host as serde::Deserialize>::deserialize(&v).unwrap()
}

fn ep(host: &str, port: u16) -> Endpoint {
    Endpoint {
        host: host.into(),
        port,
    }
}

fn code(r: Result<Value, Said>) -> Result<(), &'static str> {
    r.map(|_| ()).map_err(|s| match s.code.as_deref() {
        Some("bad_args") => "bad_args",
        Some("bad_jump") => "bad_jump",
        _ => "(other)",
    })
}

#[test]
fn address_lines_read_the_four_shapes_and_refuse_garbage() {
    assert_eq!(parse_address_line("h", 22), Some(ep("h", 22)));
    assert_eq!(parse_address_line(" h:2222 ", 22), Some(ep("h", 2222)));
    assert_eq!(parse_address_line("[::1]:2200", 22), Some(ep("::1", 2200)));
    assert_eq!(parse_address_line("fe80::1", 22), Some(ep("fe80::1", 22)));
    assert_eq!(parse_address_line(":22", 22), None, "只有端口没有 host");
    assert_eq!(
        parse_address_line("h:x", 22),
        None,
        "端口非法 ⇒ 拒，不静默默认"
    );
    assert_eq!(parse_address_line("  ", 22), None);
}

#[test]
fn the_name_is_the_trimmed_label_else_the_trimmed_host() {
    assert_eq!(m(json!({"host": " h ", "label": "  "})).name(), "h");
    assert_eq!(m(json!({"host": "h", "label": " box "})).name(), "box");
    assert_eq!(m(json!({"host": "h"})).name(), "h");
}

#[test]
fn one_machine_is_judged_by_one_rule() {
    let ok = json!({"host": "h", "user": "u"});
    assert_eq!(m(ok.clone()).fault(), None);
    let with = |k: &str, v: Value| {
        let mut o = ok.clone();
        o[k] = v;
        m(o).fault()
    };
    assert_eq!(with("host", json!(" ")), Some(MachineFaultCode::NoHost));
    assert_eq!(with("user", json!("")), Some(MachineFaultCode::NoUser));
    assert_eq!(with("port", json!(0)), Some(MachineFaultCode::Port));
    assert_eq!(with("port", json!(65536)), Some(MachineFaultCode::Port));
    assert_eq!(
        with("user", json!("-oX")),
        Some(MachineFaultCode::LeadingDash)
    );
    assert_eq!(
        with("addresses", json!(["ok", " -x"])),
        Some(MachineFaultCode::LeadingDash)
    );
    let twin = [
        m(json!({"host": "a", "user": "u", "label": "x"})),
        m(json!({"host": "b", "user": "u", "label": " x"})),
    ];
    assert_eq!(
        check_table(&twin).map(|(c, h)| (c, h.host.as_str())),
        Some((MachineFaultCode::NameTaken, "b")),
        "名字去首尾空白后比"
    );
}

#[test]
fn the_request_carries_the_form_and_no_trust_anchor() {
    let form = m(
        json!({"host": "192.0.2.2", "label": "devbox", "port": 2222, "user": "u", "keyPath": "",
                        "addresses": ["devbox.lan", "192.0.2.2:2222", "bad:x"], "jump": ""}),
    );
    let r = request(&[form], "devbox", None, "stream", json!({"stages": true})).unwrap();
    assert!(
        r.get("host_key_fingerprint").is_none() && r.get("known_keys").is_none(),
        "信哪把主机钥匙由拨号核心读库，请求里不带：{r}"
    );
    assert_eq!(r["key_path"], Value::Null, "空串 ⇒ 走 agent");
    assert_eq!(
        r["endpoints"],
        json!([{"host": "192.0.2.2", "port": 2222}, {"host": "devbox.lan", "port": 2222}]),
        "host 排首、去重、坏行丢"
    );
    assert_eq!(
        (r["use"].as_str(), r["stages"].as_bool()),
        (Some("stream"), Some(true))
    );
    serde_json::from_value::<crate::DialRequest>(r).expect("组出来的请求后端自己读得动");
}

#[test]
fn a_machine_that_does_not_hold_is_not_dialed() {
    for bad in [
        json!({"host": "h", "user": " "}),
        json!({"host": "h", "user": "u", "port": 70000}),
    ] {
        let t = [m(bad)];
        assert_eq!(
            code(request(&t, "h", None, "stream", json!({}))),
            Err("bad_args")
        );
    }
    let t = [
        m(json!({"host": "h", "user": "u", "jump": "j"})),
        m(json!({"host": "j", "user": ""})),
    ];
    assert_eq!(
        code(request(&t, "h", None, "stream", json!({}))),
        Err("bad_args"),
        "跳板那一台不成立也拒"
    );
    let t = [m(json!({"host": "h", "user": "u"}))];
    assert_eq!(
        code(request(&t, "nobody", None, "stream", json!({}))),
        Err("bad_args"),
        "表里没有这一台"
    );
}

#[test]
fn jumps_are_followed_through_the_table_hop_by_hop() {
    let t = [
        m(json!({"host": "192.0.2.2", "label": "devbox", "user": "u", "jump": "bastion"})),
        m(
            json!({"host": "b.example", "label": "bastion", "user": "j", "keyPath": "/k", "jump": "edge"}),
        ),
        m(json!({"host": "e.example", "label": "edge", "user": "e"})),
    ];
    let r = request(&t, "devbox", None, "forward", json!({})).unwrap();
    assert_eq!(
        (
            r["jump"]["host"].as_str(),
            r["jump"]["label"].as_str(),
            r["jump"]["key_path"].as_str()
        ),
        (Some("b.example"), Some("bastion"), Some("/k"))
    );
    assert_eq!(
        r["jump"]["jump"]["host"], "e.example",
        "跳板自己的跳板再套一层"
    );
    assert!(r["jump"]["jump"].get("jump").is_none());
    serde_json::from_value::<crate::DialRequest>(r).expect("多跳那一形读得动");
}

#[test]
fn a_jump_that_is_missing_or_points_back_is_refused() {
    let via = m(json!({"host": "192.0.2.2", "label": "devbox", "user": "u", "jump": "bastion"}));
    assert_eq!(
        code(request(
            &[via.clone()],
            "devbox",
            None,
            "forward",
            json!({})
        )),
        Err("bad_jump"),
        "跳板不在表里 ⇒ 拒（不回落直连）"
    );
    let wrong = m(json!({"host": "x", "label": "other", "user": "j"}));
    assert_eq!(
        code(request(
            &[via.clone(), wrong],
            "devbox",
            None,
            "forward",
            json!({})
        )),
        Err("bad_jump")
    );
    let far = m(json!({"host": "b", "label": "bastion", "user": "j", "jump": "elsewhere"}));
    assert_eq!(
        code(request(&[via, far], "devbox", None, "forward", json!({}))),
        Err("bad_jump"),
        "再外一台找不到 ⇒ 也拒（不再忽略跳板自己的跳板）"
    );
    let selfish =
        m(json!({"host": "192.0.2.2", "label": "devbox", "user": "u", "jump": " devbox "}));
    assert_eq!(
        code(request(&[selfish], "devbox", None, "forward", json!({}))),
        Err("bad_jump"),
        "环"
    );
}

#[test]
fn a_jump_chain_that_loops_back_is_refused() {
    // 表 A→B→A：连 A 经 B，B 又经 A ⇒ 环，拒（不回落成「B 的跳板忽略」）。
    let t = [
        m(json!({"host": "a.example", "label": "A", "user": "u", "jump": "B"})),
        m(json!({"host": "b.example", "label": "B", "user": "u", "jump": "A"})),
    ];
    assert_eq!(
        code(request(&t, "A", None, "stream", json!({}))),
        Err("bad_jump")
    );
}

#[test]
fn the_preferred_winner_goes_first_while_it_is_still_one_of_the_addresses() {
    // 组法只在这里：表里那一台 ＋ 上次赢的那条（后端自己记的）⇒ 拨号请求，其余格原样进请求。
    let table = vec![m(
        json!({"label": "devbox", "host": "192.0.2.2", "user": "u",
        "addresses": ["devbox.lan", "[fe80::1]:2200"]}),
    )];
    let extra = json!({"command": "x", "agent_sock": "/run/agent"});
    let r = req(
        &table,
        "devbox",
        Some(&ep("fe80::1", 2200)),
        "capture",
        extra.clone(),
    )
    .unwrap();
    let order: Vec<(String, u16)> = r
        .race_order()
        .into_iter()
        .map(|e| (e.host, e.port))
        .collect();
    assert_eq!(
        order,
        vec![
            ("fe80::1".into(), 2200),
            ("192.0.2.2".into(), 22),
            ("devbox.lan".into(), 22)
        ],
        "上次赢的那条排首，其余保序"
    );
    assert_eq!(r.use_, crate::Use::Capture);
    assert_eq!(
        (r.command.as_str(), r.agent_sock.as_deref()),
        ("x", Some("/run/agent"))
    );
    // 上次赢的那条已不在这台的地址里（配置改过）⇒ 顺序不动。
    let order: Vec<String> = req(
        &table,
        "devbox",
        Some(&ep("gone.lan", 22)),
        "capture",
        extra,
    )
    .unwrap()
    .race_order()
    .into_iter()
    .map(|e| e.host)
    .collect();
    assert_eq!(order, vec!["192.0.2.2", "devbox.lan", "fe80::1"]);
}

/// 组好之后读成拨号请求（后端拿 [`request`] 那一形读，同一份）。
fn req(
    table: &[Host],
    name: &str,
    prefer: Option<&Endpoint>,
    use_: &str,
    extra: Value,
) -> Result<crate::DialRequest, Said> {
    let v = request(table, name, prefer, use_, extra)?;
    Ok(serde_json::from_value(v).expect("组好的那一形读得成拨号请求"))
}

/// 机器表那一道没过的那一句：格下那一句（添加机器框 · 机器卡同读）与写口拒绝那一句（带名字与「未保存」），逐码对文案表。
#[test]
fn faults_say_the_line_for_their_code() {
    use MachineFaultCode as K;
    for (code, key) in [
        (K::NoHost, "addMachine.err.host"),
        (K::NoUser, "addMachine.err.user"),
        (K::Port, "addMachine.err.port"),
        (K::LeadingDash, "addMachine.err.dash"),
    ] {
        let f = fault(code, "a".into());
        assert_eq!((f.code, f.name.as_str()), (code, "a"));
        assert_eq!(f.said, copy_text(key, &[]), "{code:?}");
    }
    assert_eq!(
        fault(K::NameTaken, "B".into()).said,
        copy_text("addMachine.err.taken", &[("name", "B")])
    );
    for (code, key) in [
        (K::NoHost, "rsConfig.machine.noHost"),
        (K::NoUser, "rsConfig.machine.noUser"),
        (K::Port, "rsConfig.machine.port"),
        (K::NameTaken, "rsConfig.machine.nameTaken"),
        (K::LeadingDash, "rsConfig.machine.dash"),
    ] {
        let said = refused_said(&fault(code, "B".into()));
        assert_eq!(said, copy_text(key, &[("name", "B")]), "{code:?}");
        assert!(said.contains('B'), "写口拒绝那一句没带那台的名字：{said}");
    }
}
