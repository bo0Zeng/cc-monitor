//! 离线那台的上次值：本机后端替界面记下每台最近一次读成的那几份，跨重启还在。

use super::*;
use serde_json::json;

/// 判据仍用 JSON 写入参：经 `typed::take` 解成入参类型（同帧面那一跳），回包序列化回 JSON 比。
fn read_at(
    p: &std::path::Path,
    a: &serde_json::Value,
) -> Result<serde_json::Value, crate::wire::spec::Fail> {
    let req = crate::wire::typed::take(a)?;
    super::read_at(p, &req).map(|x| serde_json::to_value(x).unwrap())
}
fn write_at(
    own: &crate::platform::fs::Own,
    a: &serde_json::Value,
    now: u64,
) -> Result<serde_json::Value, crate::wire::spec::Fail> {
    let req = crate::wire::typed::take(a)?;
    super::write_at(own, &req, now).map(|x| serde_json::to_value(x).unwrap())
}

/// 一份合法的 `data-report` 应答（后端真类型序列化出来的）。
fn report() -> serde_json::Value {
    use crate::footprint::chores::{Chore, ChoreAction, ChoreKind};
    let mut c = Chore::new("ccm", ChoreKind::Must, ChoreAction::CopyCommand);
    c.go = Some(crate::footprint::data::UndoAt::page("ext"));
    serde_json::to_value(crate::footprint::data::DataReport {
        home: "/home/u".into(),
        changed_files: vec![],
        todo: vec![c],
        tmux: Some(true),
        chores: 1,
        own: vec![],
    })
    .unwrap()
}

/// 一份合法的 `accounts-list` 应答（金样里那一份，后端成品现产过）。
fn accounts_list() -> serde_json::Value {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/accounts.golden.json")).unwrap();
    g["accounts-list"].clone()
}

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ccm-last-seen-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    /// 写句柄：家目录指到这个临时目录。
    fn own(&self) -> crate::platform::fs::Own {
        crate::platform::fs::Own::issue_at(&self.0)
    }
    fn file(&self) -> std::path::PathBuf {
        self.0.join(".cc-monitor").join("last-seen.json")
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn 记下再读回_带记下的时刻_没记过的那一样是空() {
    let d = Dir::new("rw");
    let v = accounts_list();
    write_at(
        &d.own(),
        &json!({"origin": "devbox", "kind": "accounts", "value": v}),
        1000,
    )
    .unwrap();
    let got = read_at(&d.file(), &json!({"origin": "devbox"})).unwrap();
    assert_eq!(
        got,
        json!({"accounts": {"state": "kept", "atMs": 1000, "value": v}, "data": null})
    );
    let other = read_at(&d.file(), &json!({"origin": "gpu-01"})).unwrap();
    assert_eq!(other, json!({"accounts": null, "data": null}));
}

#[test]
fn 文件不在或读不懂_照没记过算_写的时候整份重来() {
    let d = Dir::new("bad");
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap(),
        json!({"accounts": null, "data": null})
    );
    std::fs::create_dir_all(d.file().parent().unwrap()).unwrap();
    std::fs::write(d.file(), b"{not json").unwrap();
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap(),
        json!({"accounts": null, "data": null})
    );
    write_at(
        &d.own(),
        &json!({"origin": "devbox", "kind": "accounts", "value": accounts_list()}),
        5,
    )
    .unwrap();
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap()["accounts"],
        json!({"state": "kept", "atMs": 5, "value": accounts_list()})
    );
}

#[test]
fn 入参闭集_种类只认两样_值要是对象_太大的不收() {
    let d = Dir::new("args");
    let code = |a: serde_json::Value| write_at(&d.own(), &a, 1).unwrap_err().code;
    assert_eq!(
        code(json!({"origin": "devbox", "kind": "ext", "value": {}})),
        "bad_args"
    );
    assert_eq!(
        code(json!({"origin": "", "kind": "data", "value": {}})),
        "bad_args"
    );
    assert_eq!(
        code(json!({"origin": "devbox", "kind": "data", "value": [1]})),
        "bad_args"
    );
    let big = "x".repeat(MAX_VALUE_BYTES + 1);
    assert_eq!(
        code(json!({"origin": "devbox", "kind": "data", "value": {"s": big}})),
        "too_large"
    );
    assert_eq!(read_at(&d.file(), &json!({})).unwrap_err().code, "bad_args");
    assert!(!d.file().exists(), "拒掉的不许落一个字节");
}

#[test]
fn 台数封顶_超了先丢最久没更新的那台() {
    let d = Dir::new("cap");
    for i in 0..=MAX_ORIGINS {
        write_at(
            &d.own(),
            &json!({"origin": format!("m{i}"), "kind": "data", "value": report()}),
            i as u64 + 1,
        )
        .unwrap();
    }
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "m0"})).unwrap()["data"],
        serde_json::Value::Null
    );
    assert!(
        !read_at(&d.file(), &json!({"origin": format!("m{MAX_ORIGINS}")})).unwrap()["data"]
            .is_null()
    );
}

#[test]
fn 删机器时清掉那台_两样都读不到_别的台不动_没记过的那台照样回成() {
    let d = Dir::new("forget");
    for (o, k) in [
        ("devbox", "accounts"),
        ("devbox", "data"),
        ("gpu-01", "data"),
    ] {
        write_at(
            &d.own(),
            &json!({"origin": o, "kind": k, "value": if k == "data" { report() } else { accounts_list() }}),
            10,
        )
        .unwrap();
    }
    write_at(&d.own(), &json!({"origin": "devbox", "forget": true}), 20).unwrap();
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap(),
        json!({"accounts": null, "data": null})
    );
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "gpu-01"})).unwrap(),
        json!({"accounts": null, "data": {"state": "kept", "atMs": 10, "value": report()}})
    );
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(d.file()).unwrap()).unwrap();
    assert!(raw.get("devbox").is_none(), "盘上还留着那台：{raw}");
    write_at(&d.own(), &json!({"origin": "never", "forget": true}), 30).unwrap();
    // 清就只清：带着 kind / value 的 forget 不收（两件事别混在一次里）。
    let mixed = write_at(
        &d.own(),
        &json!({"origin": "gpu-01", "forget": true, "kind": "data", "value": report()}),
        40,
    );
    assert_eq!(mixed.unwrap_err().code, "bad_args");
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "gpu-01"})).unwrap()["data"]["atMs"],
        10
    );
}

/// 每一样按它那条命令的回包类型收：多一格 · 缺一格 · 类型不对 ⇒ `bad_args`、不落盘；收下的连同那条命令此刻的形状指纹一起存。
#[test]
fn 每一样按回包类型收_形状不对的不收() {
    let d = Dir::new("shape-in");
    let code = |kind: &str, v: serde_json::Value| {
        write_at(
            &d.own(),
            &json!({"origin": "devbox", "kind": kind, "value": v}),
            1,
        )
        .unwrap_err()
        .code
    };
    let mut extra = report();
    extra["extra"] = json!(1);
    let mut missing = report();
    missing.as_object_mut().unwrap().remove("own");
    let mut wrong = report();
    wrong["todo"][0]["kind"] = json!("maybe");
    for v in [json!({"home": "/h"}), extra, missing, wrong] {
        assert_eq!(code("data", v), "bad_args");
    }
    let mut extra = accounts_list();
    extra["meta"]["extra"] = json!(1);
    let mut missing = accounts_list();
    missing["accounts"][0]
        .as_object_mut()
        .unwrap()
        .remove("color");
    let mut wrong = accounts_list();
    wrong["accounts"][0]["color"] = json!("3");
    for v in [
        json!({"meta": {"enabled": true}, "accounts": []}),
        extra,
        missing,
        wrong,
    ] {
        assert_eq!(code("accounts", v), "bad_args");
    }
    assert!(!d.file().exists(), "拒掉的不许落一个字节");
    write_at(
        &d.own(),
        &json!({"origin": "devbox", "kind": "data", "value": report()}),
        7,
    )
    .unwrap();
    write_at(
        &d.own(),
        &json!({"origin": "devbox", "kind": "accounts", "value": accounts_list()}),
        8,
    )
    .unwrap();
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(d.file()).unwrap()).unwrap();
    for (kind, cmd) in [("data", "data-report"), ("accounts", "accounts-list")] {
        assert_eq!(
            raw["devbox"][kind]["shape"],
            json!(cmd_wire::shapes::shape_of(cmd).unwrap()),
            "{kind}：存档要记下是哪一版的回包"
        );
    }
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap()["data"],
        json!({"state": "kept", "atMs": 7, "value": report()})
    );
}

/// 存档不是此刻这一版（指纹不同 · 旧版存的没带指纹 · 解不回回包类型）⇒ 照实说读不懂，原样不交出去。
#[test]
fn 存档不是这一版_照实说读不懂_不交原样() {
    let d = Dir::new("shape-out");
    std::fs::create_dir_all(d.file().parent().unwrap()).unwrap();
    let mut wrong = report();
    wrong["tmux"] = json!("no");
    let now = cmd_wire::shapes::shape_of("data-report").unwrap();
    let mut bent = accounts_list();
    bent["notice"] = json!(1);
    let now_accounts = cmd_wire::shapes::shape_of("accounts-list").unwrap();
    for (o, kind, entry) in [
        (
            "old",
            "data",
            json!({"atMs": 3, "shape": "00000000", "value": report()}),
        ),
        ("bare", "data", json!({"atMs": 4, "value": report()})),
        (
            "bent",
            "data",
            json!({"atMs": 5, "shape": now, "value": wrong}),
        ),
        // 旧版记的账号清单没带指纹（那时原样存）⇒ 照样读不懂。
        (
            "bare-accounts",
            "accounts",
            json!({"atMs": 6, "value": accounts_list()}),
        ),
        (
            "bent-accounts",
            "accounts",
            json!({"atMs": 7, "shape": now_accounts, "value": bent}),
        ),
    ] {
        std::fs::write(d.file(), json!({ o: {kind: entry} }).to_string()).unwrap();
        let got = read_at(&d.file(), &json!({"origin": o})).unwrap();
        assert_eq!(got[kind]["state"], json!("unreadable"), "{o}: {got}");
        assert!(
            got[kind].get("value").is_none(),
            "{o}：读不懂的原样不交出去"
        );
    }
}

/// 同一份再交 ⇒ 内容不变、时刻换成这一次（离线时「上次的」说的是最后一次看到它）；回包也是这一次。内容变了照常记。
#[test]
fn 同一份再交_内容不变_时刻是新的() {
    let d = Dir::new("same");
    let put = |v: &serde_json::Value, kind: &str, now: u64| {
        write_at(
            &d.own(),
            &json!({"origin": "devbox", "kind": kind, "value": v}),
            now,
        )
        .unwrap()
    };
    let acc = json!({"meta": {"enabled": true}, "accounts": []});
    put(&acc, "accounts", 10);
    put(&report(), "data", 11);
    assert_eq!(put(&acc, "accounts", 20), json!({"atMs": 20}));
    assert_eq!(put(&report(), "data", 21), json!({"atMs": 21}));
    let got = read_at(&d.file(), &json!({"origin": "devbox"})).unwrap();
    assert_eq!(got["accounts"], json!({"atMs": 20, "value": acc}));
    assert_eq!(
        got["data"],
        json!({"state": "kept", "atMs": 21, "value": report()})
    );
    let changed = json!({"meta": {"enabled": false}, "accounts": []});
    assert_eq!(put(&changed, "accounts", 30), json!({"atMs": 30}));
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap()["accounts"],
        json!({"atMs": 30, "value": changed})
    );
}
