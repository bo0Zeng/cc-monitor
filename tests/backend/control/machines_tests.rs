//! 机器表（`machines.json`）：读的三态 · 补丁只动认出的那一台那一格 · 判准（名字不重 · 端口 · 地址 · 用户）写口与试算口同一份 ·
//! 地址与用户不以 `-` 打头 · 认不出 / 已在整批拒 · 读不懂不盖 · 写完响一下 · 忘掉一台的主机指纹。家目录都在临时目录里。
use super::*;
use serde_json::json;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-machines-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join(".cc-monitor")).unwrap();
    d
}

fn edits(v: Value) -> MachinesEdits {
    serde_json::from_value(json!({ "edits": v })).unwrap()
}

fn set(home: &Path, v: Value) -> Result<MachinesSet, MachinesSetErr> {
    let own = Own::issue_at(home);
    answer_set_in(Some(home), &own, &edits(v))
}

fn by_name(k: &str) -> Value {
    json!([{ "fields": ["label", "host"], "equals": k }])
}

fn insert(label: &str, host: &str) -> Value {
    json!({ "op": "insertin", "where": by_name(if label.is_empty() { host } else { label }),
            "value": { "label": label, "host": host, "user": "pi", "port": 22 } })
}

fn code(e: MachinesSetErr) -> String {
    use crate::wire::typed::Codes;
    e.into_fail().code.to_string()
}

/// 没有那份 ⇒ `absent`、空表；加两台 ⇒ `ok`、照序读回；改一格只动认出的那一台那一格。
#[test]
fn machines_start_absent_and_a_cell_edit_touches_only_that_cell() {
    let home = scratch("cell");
    let r = answer_read_in(Some(&home));
    assert_eq!((r.state, r.hosts.len()), ("absent", 0));
    set(&home, json!([insert("gpu", "g.lan"), insert("", "pi.lan")])).unwrap();
    let r = answer_read_in(Some(&home));
    assert_eq!(r.state, "ok");
    assert_eq!(
        r.hosts.iter().map(Host::name).collect::<Vec<_>>(),
        ["gpu", "pi.lan"]
    );
    assert!(r.hosts.iter().all(|h| h.connect), "缺 ⇒ 连");
    let done = set(
        &home,
        json!([{ "op": "setin", "where": by_name("pi.lan"), "field": "jump", "value": "gpu" }]),
    )
    .unwrap();
    assert_eq!(done.machines.hosts[1].jump, "gpu");
    assert_eq!(done.machines.hosts[0], r.hosts[0], "别的那一台动了");
    let _ = std::fs::remove_dir_all(&home);
}

/// 认不出 / 认出多台 ⇒ `element_gone`；要加的已在 ⇒ `element_exists`；改完的表不成立 ⇒ `bad_machine` ＋ 那一处。都一个字节不写。
#[test]
fn a_bad_batch_is_refused_whole_and_writes_nothing() {
    let home = scratch("refuse");
    set(
        &home,
        json!([insert("gpu", "g.lan"), insert("pi", "p.lan")]),
    )
    .unwrap();
    let file = path_in(&home);
    let before = std::fs::read(&file).unwrap();
    let cases = [
        (
            json!([{ "op": "removein", "where": by_name("nope") }]),
            "element_gone",
        ),
        (json!([insert("gpu", "x.lan")]), "element_exists"),
        (
            json!([{ "op": "setin", "where": by_name("pi"), "field": "label", "value": "gpu" }]),
            "bad_machine",
        ),
        (
            json!([{ "op": "setin", "where": by_name("pi"), "field": "port", "value": 70000 }]),
            "bad_machine",
        ),
        (
            json!([{ "op": "setin", "where": by_name("pi"), "field": "resumeCommand", "value": "cc" }]),
            "bad_args",
        ),
        (json!([{ "op": "removein", "where": [] }]), "bad_args"),
    ];
    for (e, want) in cases {
        let got = set(&home, e.clone()).unwrap_err();
        assert_eq!(code(got), want, "{e}");
        assert_eq!(std::fs::read(&file).unwrap(), before, "{e} 写了盘");
    }
    let MachinesSetErr::BadMachine(_, f) = set(
        &home,
        json!([{ "op": "setin", "where": by_name("pi"), "field": "user", "value": " " }]),
    )
    .unwrap_err() else {
        panic!("不是 bad_machine")
    };
    assert_eq!((f.code, f.name.as_str()), (MachineFaultCode::NoUser, "pi"));
    let _ = std::fs::remove_dir_all(&home);
}

/// 试算口与写口同一份判准、盘上一个字节不动；同名那一形说成 `name_taken`。
#[test]
fn trying_says_the_same_fault_and_never_writes() {
    let home = scratch("try");
    set(&home, json!([insert("gpu", "g.lan")])).unwrap();
    let before = std::fs::read(path_in(&home)).unwrap();
    let t = |v: Value| answer_try_in(Some(&home), &edits(v)).fault.map(|f| f.code);
    assert_eq!(
        t(json!([insert("gpu", "o.lan")])),
        Some(MachineFaultCode::NameTaken)
    );
    assert_eq!(t(json!([insert("", "")])), Some(MachineFaultCode::NoHost));
    assert_eq!(t(json!([insert("pi", "p.lan")])), None);
    assert_eq!(
        t(json!([{ "op": "removein", "where": by_name("nope") }])),
        None
    );
    assert_eq!(std::fs::read(path_in(&home)).unwrap(), before);
    let _ = std::fs::remove_dir_all(&home);
}

/// 读不懂（不是 JSON · 版本不认 · 多出来的键）⇒ `unreadable`、空表；写口拒，原文不动。
#[test]
fn an_unreadable_table_is_reported_and_never_overwritten() {
    let home = scratch("bad");
    for text in [
        "{ not json",
        r#"{"version": 9, "hosts": []}"#,
        r#"{"version": 1, "hosts": [{"host": "h", "resumeCommand": "cc"}]}"#,
    ] {
        std::fs::write(path_in(&home), text).unwrap();
        let r = answer_read_in(Some(&home));
        assert_eq!((r.state, r.hosts.len()), ("unreadable", 0), "{text}");
        assert!(r.reason.is_some());
        assert_eq!(
            code(set(&home, json!([insert("gpu", "g.lan")])).unwrap_err()),
            "unreadable"
        );
        assert_eq!(std::fs::read_to_string(path_in(&home)).unwrap(), text);
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 写完响一下（流连接照它推 `changed {machines}`）；一条都没动 ⇒ 不写。
#[test]
fn a_write_rings_the_bell_and_a_no_op_does_not() {
    let home = scratch("bell");
    set(&home, json!([insert("gpu", "g.lan")])).unwrap();
    let mut rx = changes().subscribe();
    set(
        &home,
        json!([{ "op": "setin", "where": by_name("gpu"), "field": "connect", "value": false }]),
    )
    .unwrap();
    assert!(rx.try_recv().is_ok(), "写了没响");
    set(&home, json!([])).unwrap();
    // 别的判据并行写也会响：只钉「没动的那一趟没写盘」。
    assert!(!answer_read_in(Some(&home)).hosts[0].connect);
    assert_eq!(answer_read_in(Some(&home)).hosts[0].user, "pi");
    let _ = std::fs::remove_dir_all(&home);
}

/// 地址（含备用地址）与用户以 `-` 打头 ⇒ 写口与试算口都拒（开终端时它们是拼进 ssh 那一行的裸词）；正控：中间带 `-` 的照收。
#[test]
fn a_leading_dash_in_an_address_or_user_is_refused() {
    let home = scratch("dash");
    let mut bad_user = insert("a", "a.lan");
    bad_user["value"]["user"] = json!("-oProxyCommand=x");
    let mut bad_alt = insert("b", "b.lan");
    bad_alt["value"]["addresses"] = json!(["ok.lan", " -oX"]);
    for e in [
        json!([insert("", "-x.lan")]),
        json!([bad_user]),
        json!([bad_alt]),
    ] {
        let MachinesSetErr::BadMachine(_, f) = set(&home, e.clone()).unwrap_err() else {
            panic!("{e}")
        };
        assert_eq!(f.code, MachineFaultCode::LeadingDash, "{e}");
        assert_eq!(
            answer_try_in(Some(&home), &edits(e.clone()))
                .fault
                .map(|f| f.code),
            Some(MachineFaultCode::LeadingDash),
            "{e}"
        );
    }
    set(&home, json!([insert("c", "my-host.lan")])).unwrap();
    let edit =
        json!([{ "op": "setin", "where": by_name("c"), "field": "addresses", "value": ["-y"] }]);
    assert!(
        matches!(set(&home, edit), Err(MachinesSetErr::BadMachine(..))),
        "改备用地址也要过那一道"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 读机器表时带出库里每台记着的指纹（按这台全部地址认）；忘掉一台只删它的行、别的台留着；表里没有这台 ⇒ `element_gone`。
#[test]
fn reading_shows_each_hosts_keys_and_forgetting_drops_only_that_one() {
    let home = scratch("forget");
    let key = |seed: u8| {
        let k = russh::keys::PrivateKey::from(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[seed; 32]),
        );
        let l = k.public_key().to_openssh().unwrap();
        (
            l.split_whitespace().take(2).collect::<Vec<_>>().join(" "),
            ssh_dial::known_hosts::fingerprint(k.public_key()),
        )
    };
    let (ka, fa) = key(1);
    let (kb, fb) = key(2);
    let mut a = insert("gpu", "g.lan");
    a["value"]["addresses"] = json!(["192.0.2.5:2222"]);
    set(&home, json!([a, insert("pi", "pi.lan")])).unwrap();
    std::fs::write(
        home.join(".cc-monitor/known_hosts"),
        format!("[192.0.2.5]:2222 {ka}\npi.lan {kb}\nelse {kb}\n"),
    )
    .unwrap();
    let r = answer_read_in(Some(&home));
    assert_eq!(
        r.host_keys.get("gpu"),
        Some(&vec![fa]),
        "备用地址那一行也算这台的"
    );
    assert_eq!(r.host_keys.get("pi"), Some(&vec![fb.clone()]));
    let own = Own::issue_at(&home);
    let req = |m: &str| HostKeysForgetReq {
        machine: cmd_wire::Machine(m.to_string()),
    };
    let after = answer_forget_in(Some(&home), &own, &req("gpu")).unwrap();
    assert_eq!(after.host_keys.get("gpu"), None);
    assert_eq!(after.host_keys.get("pi"), Some(&vec![fb]));
    assert_eq!(
        std::fs::read_to_string(home.join(".cc-monitor/known_hosts")).unwrap(),
        format!("pi.lan {kb}\nelse {kb}\n")
    );
    assert!(matches!(
        answer_forget_in(Some(&home), &own, &req("nope")),
        Err(HostKeysForgetErr::ElementGone(_))
    ));
    let _ = std::fs::remove_dir_all(&home);
}
