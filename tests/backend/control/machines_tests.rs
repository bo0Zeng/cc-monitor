//! 机器表（`machines.json`）：读的三态 · 补丁只动认出的那一台那一格 · 判准（名字不重 · 端口 · 地址 · 用户）写口与试算口同一份 ·
//! 日子只由写口盖 · `ifEmpty` 不盖已有的指纹 · 认不出 / 已在整批拒 · 读不懂不盖 · 写完响一下。家目录都在临时目录里。
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
    answer_set_in(Some(home), &own, &edits(v), "2026-10-13")
}

fn by_name(k: &str) -> Value {
    json!([{ "fields": ["label", "host"], "equals": k }])
}

fn insert(label: &str, host: &str) -> Value {
    json!({ "op": "insertin", "where": by_name(if label.is_empty() { host } else { label }),
            "value": { "label": label, "host": host, "user": "pi", "port": 22 } })
}

fn code(e: MachinesSetErr) -> String {
    use crate::stream::inbound::typed::Codes;
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
    assert_eq!(r.hosts.iter().map(Host::name).collect::<Vec<_>>(), ["gpu", "pi.lan"]);
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

/// 写进指纹 ⇒ 那一台盖上写的那一天（补丁里带来的那一格不认）；指纹清空 ⇒ 日子清空；`ifEmpty` 不盖已有的指纹。
#[test]
fn the_writer_stamps_the_pinned_day_and_if_empty_keeps_a_set_fingerprint() {
    let home = scratch("pin");
    let mut add = insert("gpu", "g.lan");
    add["value"]["hostKeyFingerprint"] = json!("SHA256:a");
    add["value"]["hostKeyPinnedAt"] = json!("1999-01-01");
    let done = set(&home, json!([add])).unwrap();
    assert_eq!(done.pinned, [PinnedDay { origin: "gpu".into(), day: "2026-10-13".into() }]);
    assert_eq!(done.machines.hosts[0].host_key_pinned_at, "2026-10-13");
    let pin = |fp: &str| json!([{ "op": "setin", "where": by_name("gpu"), "field": "hostKeyFingerprint", "value": fp, "ifEmpty": true }]);
    let kept = set(&home, pin("SHA256:b")).unwrap();
    assert_eq!((kept.kept, kept.machines.hosts[0].host_key_fingerprint.as_str()), (1, "SHA256:a"));
    let cleared = set(
        &home,
        json!([{ "op": "setin", "where": by_name("gpu"), "field": "hostKeyFingerprint", "value": "" }]),
    )
    .unwrap();
    assert_eq!(cleared.machines.hosts[0].host_key_pinned_at, "");
    let refused = set(
        &home,
        json!([{ "op": "setin", "where": by_name("gpu"), "field": "hostKeyPinnedAt", "value": "2000-01-01" }]),
    );
    assert_eq!(code(refused.unwrap_err()), "bad_args");
    let _ = std::fs::remove_dir_all(&home);
}

/// 认不出 / 认出多台 ⇒ `element_gone`；要加的已在 ⇒ `element_exists`；改完的表不成立 ⇒ `bad_machine` ＋ 那一处。都一个字节不写。
#[test]
fn a_bad_batch_is_refused_whole_and_writes_nothing() {
    let home = scratch("refuse");
    set(&home, json!([insert("gpu", "g.lan"), insert("pi", "p.lan")])).unwrap();
    let file = path_in(&home);
    let before = std::fs::read(&file).unwrap();
    let cases = [
        (json!([{ "op": "removein", "where": by_name("nope") }]), "element_gone"),
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
    assert_eq!(t(json!([insert("gpu", "o.lan")])), Some(MachineFaultCode::NameTaken));
    assert_eq!(t(json!([insert("", "")])), Some(MachineFaultCode::NoHost));
    assert_eq!(t(json!([insert("pi", "p.lan")])), None);
    assert_eq!(t(json!([{ "op": "removein", "where": by_name("nope") }])), None);
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
        assert_eq!(code(set(&home, json!([insert("gpu", "g.lan")])).unwrap_err()), "unreadable");
        assert_eq!(std::fs::read_to_string(path_in(&home)).unwrap(), text);
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 写完响一下（流连接照它推 `changed {machines}`）；一条都没动（`ifEmpty` 没盖）⇒ 不写、不响。
#[test]
fn a_write_rings_the_bell_and_a_no_op_does_not() {
    let home = scratch("bell");
    set(&home, json!([insert("gpu", "g.lan")])).unwrap();
    let mut rx = changes().subscribe();
    set(&home, json!([{ "op": "setin", "where": by_name("gpu"), "field": "connect", "value": false }])).unwrap();
    assert!(rx.try_recv().is_ok(), "写了没响");
    set(&home, json!([{ "op": "setin", "where": by_name("gpu"), "field": "user", "value": "x", "ifEmpty": true }])).unwrap();
    // 别的判据并行写也会响：只钉「没动的那一趟没写盘」。
    assert!(!answer_read_in(Some(&home)).hosts[0].connect);
    assert_eq!(answer_read_in(Some(&home)).hosts[0].user, "pi");
    let _ = std::fs::remove_dir_all(&home);
}

/// 拨号时自动固化指纹那一问（monitor `dial_host::pin_request` 同一形）：只动认 origin 与 `host` 都对得上的那一台；
/// 已有 ⇒ `kept`；认不出 · 两台同名 ⇒ `element_gone`（`ambiguous` 分开说）；都一个字节不写。
#[test]
fn pinning_on_dial_touches_only_that_hosts_fingerprint() {
    let home = scratch("dialpin");
    let pin = |origin: &str, host: &str| {
        json!([{ "op": "setin",
            "where": [{ "fields": ["label", "host"], "equals": origin }, { "fields": ["host"], "equals": host }],
            "field": "hostKeyFingerprint", "value": "SHA256:new", "ifEmpty": true }])
    };
    let mut old = insert("", "h2");
    old["value"]["hostKeyFingerprint"] = json!("SHA256:old");
    set(&home, json!([insert("devbox", "h1"), old])).unwrap();
    let before = answer_read_in(Some(&home)).hosts;
    let done = set(&home, pin("devbox", "h1")).unwrap();
    assert_eq!((done.kept, done.pinned.len()), (0, 1));
    assert_eq!(done.machines.hosts[0].host_key_fingerprint, "SHA256:new");
    assert_eq!(done.machines.hosts[1], before[1], "别的那一台动了");
    let file = std::fs::read(path_in(&home)).unwrap();
    assert_eq!(set(&home, pin("h2", "h2")).unwrap().kept, 1);
    for (o, h) in [("nope", "h9"), ("devbox", "h-other")] {
        let MachinesSetErr::ElementGone(_, g) = set(&home, pin(o, h)).unwrap_err() else {
            panic!("{o}")
        };
        assert!(!g.ambiguous, "{o}");
    }
    assert_eq!(std::fs::read(path_in(&home)).unwrap(), file, "不该写却写了");
    let _ = std::fs::remove_dir_all(&home);
}

/// 天数换算的那几个界：纪元第一天 · 闰年 2 月 29 日 · 世纪年不闰 · 跨年。
#[test]
fn pinned_day_is_the_civil_date() {
    use std::time::{Duration, UNIX_EPOCH};
    let at = |secs: u64| pinned_day(UNIX_EPOCH + Duration::from_secs(secs));
    assert_eq!(at(0), "1970-01-01");
    assert_eq!(at(951_782_400), "2000-02-29");
    assert_eq!(at(4_107_542_400), "2100-03-01");
    assert_eq!(at(1_798_761_599), "2026-12-31");
    assert_eq!(at(1_798_761_600), "2027-01-01");
}
