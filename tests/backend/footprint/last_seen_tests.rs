//! 离线那台的上次值：本机后端替界面记下每台最近一次读成的那几份，跨重启还在。

use super::*;
use serde_json::json;

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ccm-last-seen-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
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
    let v = json!({"meta": {"enabled": true}, "accounts": [{"name": "work"}]});
    write_at(
        &d.file(),
        &json!({"origin": "devbox", "kind": "accounts", "value": v}),
        1000,
    )
    .unwrap();
    let got = read_at(&d.file(), &json!({"origin": "devbox"})).unwrap();
    assert_eq!(
        got,
        json!({"accounts": {"atMs": 1000, "value": v}, "data": null})
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
        &d.file(),
        &json!({"origin": "devbox", "kind": "data", "value": {"home": "/h"}}),
        5,
    )
    .unwrap();
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap()["data"],
        json!({"atMs": 5, "value": {"home": "/h"}})
    );
}

#[test]
fn 入参闭集_种类只认两样_值要是对象_太大的不收() {
    let d = Dir::new("args");
    let code = |a: serde_json::Value| write_at(&d.file(), &a, 1).unwrap_err().0;
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
    assert_eq!(read_at(&d.file(), &json!({})).unwrap_err().0, "bad_args");
    assert!(!d.file().exists(), "拒掉的不许落一个字节");
}

#[test]
fn 台数封顶_超了先丢最久没更新的那台() {
    let d = Dir::new("cap");
    for i in 0..=MAX_ORIGINS {
        write_at(
            &d.file(),
            &json!({"origin": format!("m{i}"), "kind": "data", "value": {}}),
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
            &d.file(),
            &json!({"origin": o, "kind": k, "value": {"x": 1}}),
            10,
        )
        .unwrap();
    }
    write_at(&d.file(), &json!({"origin": "devbox", "forget": true}), 20).unwrap();
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "devbox"})).unwrap(),
        json!({"accounts": null, "data": null})
    );
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "gpu-01"})).unwrap(),
        json!({"accounts": null, "data": {"atMs": 10, "value": {"x": 1}}})
    );
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(d.file()).unwrap()).unwrap();
    assert!(raw.get("devbox").is_none(), "盘上还留着那台：{raw}");
    write_at(&d.file(), &json!({"origin": "never", "forget": true}), 30).unwrap();
    // 清就只清：带着 kind / value 的 forget 不收（两件事别混在一次里）。
    let mixed = write_at(
        &d.file(),
        &json!({"origin": "gpu-01", "forget": true, "kind": "data", "value": {"x": 2}}),
        40,
    );
    assert_eq!(mixed.unwrap_err().0, "bad_args");
    assert_eq!(
        read_at(&d.file(), &json!({"origin": "gpu-01"})).unwrap()["data"]["atMs"],
        10
    );
}
