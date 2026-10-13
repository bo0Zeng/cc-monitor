//! 资产目录那一族（`assets-catalog` · `assets-catalog-merge` · `ext-note-set` · `skill-install-record`）的回包是线上形：
//! 前两条壳读、后两条界面读 / 本机后端自己经写口读。换形之前用旧代码跑出字节、钉成字面量；换形之后逐字节比
//! （经 `serde_json::Value` 那一跳，同帧面：键按字典序）。临时目录换成 `<D>`。没有重写开关：对不上就是线上形变了。

use serde_json::{json, Value};

/// 入参仍用 JSON 写：经 `typed::take` 解成入参类型（同帧面那一跳）。
fn take<R: serde::de::DeserializeOwned>(a: Value) -> R {
    crate::stream::inbound::typed::take(&a).unwrap()
}

/// 帧面那一跳的写法（键按字典序）。
fn wire(v: &impl serde::Serialize) -> String {
    serde_json::to_string(&serde_json::to_value(v).unwrap()).unwrap()
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("assets-cat-wire-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

use super::asset_catalog::{Asset, Catalog, Note, Snapshot};

/// 线上 `catalog`（JSON）→ 各台快照：同 `assets-catalog-merge` 那一跳（解码器管形状，`machines_of` 判 id 与种类）。
fn machines_from_wire(
    v: &serde_json::Value,
) -> Result<std::collections::BTreeMap<String, crate::assets::asset_catalog::Snapshot>, String> {
    let cat: crate::assets::asset_catalog::CatalogIn =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    crate::assets::asset_catalog::machines_of(cat.machines)
}

fn sample() -> Catalog {
    let mut me = Snapshot {
        label: "me@box".into(),
        gen: 2,
        seen_at: 1002,
        assets: vec![Asset {
            kind: "skill".into(),
            name: "a".into(),
            project: None,
            dir: Some("/s/a".into()),
            digest: "d-a".into(),
            summary: json!({"files": 1}),
        }],
        project_dirs: vec!["/p".into()],
        notes: Default::default(),
        shared_mcp: true,
    };
    me.notes.insert(
        "skill:a".into(),
        Note {
            text: "hi".into(),
            rev: 1,
        },
    );
    let r = Snapshot {
        label: "r@far".into(),
        gen: 5,
        seen_at: 1005,
        assets: vec![
            Asset {
                kind: "skill".into(),
                name: "a".into(),
                project: None,
                dir: None,
                digest: "d-a2".into(),
                summary: json!({}),
            },
            Asset {
                kind: "mcp".into(),
                name: "m".into(),
                project: Some("/p".into()),
                dir: Some("/p/.mcp.json".into()),
                digest: "d-m".into(),
                summary: json!({"command": "x"}),
            },
        ],
        project_dirs: vec![],
        notes: Default::default(),
        shared_mcp: false,
    };
    let mut c = super::asset_catalog::fresh("me".into());
    c.machines.insert("me".into(), me);
    c.machines.insert("r".into(), r);
    c
}

const CATALOG_WIRE: &str = r#"{"changed":true,"machines":[{"assets":[{"digest":"d-a","dir":"/s/a","kind":"skill","name":"a","summary":{"files":1}}],"gen":2,"id":"me","label":"me@box","notes":{"skill:a":{"rev":1,"text":"hi"}},"projectDirs":["/p"],"seenAt":1002,"sharedMcp":true},{"assets":[{"digest":"d-a2","kind":"skill","name":"a","summary":{}},{"digest":"d-m","dir":"/p/.mcp.json","kind":"mcp","name":"m","project":"/p","summary":{"command":"x"}}],"gen":5,"id":"r","label":"r@far","notes":{},"projectDirs":[],"seenAt":1005,"sharedMcp":false}],"path":"/x/assets-catalog.json","problems":["p1"],"rows":[{"from":[{"digest":"d-m","machine":"r","project":"/p","summary":{"command":"x"}}],"kind":"mcp","name":"m","state":"missing"},{"from":[{"digest":"d-a2","machine":"r","project":null,"summary":{}}],"kind":"skill","name":"a","state":"differs"}],"self":"me"}"#;

#[test]
fn the_catalog_reply_keeps_its_wire_bytes() {
    let got = super::asset_catalog::wire(
        &sample(),
        &["p1".to_string()],
        true,
        Some(std::path::Path::new("/x/assets-catalog.json")),
    );
    assert_eq!(wire(&got), CATALOG_WIRE);
}

/// `assets-catalog-merge` 收的 `catalog` 是另一台的整份应答：只读它的 `machines`，别的键（`rows` · `problems` …）照收；
/// `projectDirs` / `notes` / `sharedMcp` 缺或 `null` ⇒ 空。
#[test]
fn the_merge_reads_only_the_machines_of_another_reply() {
    let mut incoming: Value = serde_json::from_str(CATALOG_WIRE).unwrap();
    incoming["machines"][1]["projectDirs"] = Value::Null;
    incoming["machines"][1]["notes"] = Value::Null;
    incoming["machines"][1]
        .as_object_mut()
        .unwrap()
        .remove("sharedMcp");
    let got = machines_from_wire(&incoming).unwrap();
    assert_eq!(got, sample().machines);
}

#[test]
fn ext_note_set_keeps_its_wire_bytes() {
    let write = |key: &str, text: &str| {
        let mut c = sample();
        super::asset_catalog::set_note(&mut c, key, text, 7);
        Ok(c)
    };
    let note = |a: Value| wire(&super::ext::answer_note(&take(a), &write).unwrap());
    assert_eq!(
        note(json!({"kind": "skill", "name": "a", "text": "  new  "})),
        r#"{"note":"new"}"#
    );
    assert_eq!(
        note(json!({"kind": "mcp", "name": "zz", "text": ""})),
        r#"{"note":null}"#
    );
}

#[test]
fn skill_install_record_keeps_its_wire_bytes() {
    let d = tmp("ledger");
    let own = crate::platform::fs::Own::issue_at(&d);
    let root = d.join("skills");
    let rec = |a: Value| {
        let got = wire(&super::skill_ledger::record_at(&own, Some(&root), &take(a)).unwrap());
        got.replace(&d.display().to_string(), "<D>")
    };
    let digest = "0123456789abcdef";
    assert_eq!(
        rec(
            json!({"op": "add", "name": "s", "files": {"a.md": {"digest": digest, "created": true}, "b.md": {"digest": digest, "created": false}}})
        ),
        r#"{"changed":true,"dir":"<D>/skills/s","name":"s","remaining":2}"#
    );
    assert_eq!(
        rec(
            json!({"op": "add", "name": "s", "files": {"a.md": {"digest": digest, "created": true}}})
        ),
        r#"{"changed":false,"dir":"<D>/skills/s","name":"s","remaining":2}"#
    );
    assert_eq!(
        rec(
            json!({"op": "add", "name": "h", "at": "home", "files": {"x/h.md": {"digest": digest, "created": true}}})
        ),
        r#"{"changed":true,"dir":"<D>","name":"h","remaining":1}"#
    );
    assert_eq!(
        rec(json!({"op": "drop", "dir": root.join("s"), "paths": ["a.md"]})),
        r#"{"changed":true,"dir":"<D>/skills/s","name":"s","remaining":1}"#
    );
    assert_eq!(
        rec(json!({"op": "drop", "dir": root.join("s"), "paths": []})),
        r#"{"changed":false,"dir":"<D>/skills/s","name":"s","remaining":1}"#
    );
    assert_eq!(
        rec(json!({"op": "mcp-add", "file": "/p/.mcp.json", "name": "m", "digest": digest})),
        r#"{"changed":true,"dir":"/p/.mcp.json","name":"m","remaining":1}"#
    );
    assert_eq!(
        rec(json!({"op": "mcp-drop", "file": "/p/.mcp.json", "name": "m"})),
        r#"{"changed":true,"dir":"/p/.mcp.json","name":"m","remaining":0}"#
    );
    assert_eq!(
        rec(json!({"op": "mcp-drop", "file": "/p/.mcp.json", "name": "m"})),
        r#"{"changed":false,"dir":"/p/.mcp.json","name":"m","remaining":0}"#
    );
    let _ = std::fs::remove_dir_all(&d);
}
/// 帧面那一跳（按名找命令、照它的档跑）：入参多一个键 ⇒ `bad_args`、一个字节不写（原来不管多出来的键）。
#[test]
fn an_extra_key_is_bad_args_on_the_frame_path() {
    use crate::stream::inbound::spec::{prepare, Prepared};
    let d = tmp("extra");
    let own = crate::platform::fs::Own::issue_at(&d);
    let digest = "0123456789abcdef";
    let cases = [
        ("assets-catalog", json!({ "bogus": 1 })),
        ("mcp-read", json!({ "projectDir": null, "bogus": 1 })),
        ("hooks-diag", json!({ "bogus": 1 })),
        ("ext-list", json!({ "visit": false, "bogus": 1 })),
        ("ext-list-here", json!({ "visit": false, "bogus": 1 })),
        (
            "ext-uninstall-preview",
            json!({ "kind": "skill", "name": "a", "at": { "level": "user" }, "bogus": 1 }),
        ),
        (
            "ext-uninstall-apply",
            json!({ "kind": "skill", "name": "a", "at": { "level": "user" }, "token": "t", "bogus": 1 }),
        ),
        (
            "assets-catalog-merge",
            json!({ "catalog": { "machines": [] }, "bogus": 1 }),
        ),
        (
            "ext-note-set",
            json!({ "kind": "skill", "name": "a", "text": "t", "bogus": 1 }),
        ),
        (
            "skill-install-record",
            json!({ "op": "mcp-add", "file": "/p/.mcp.json", "name": "m", "digest": digest, "bogus": 1 }),
        ),
    ];
    for (cmd, args) in cases {
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == cmd)
            .unwrap();
        let Some(Prepared::Blocking(run)) = prepare(spec.run, Some(&own)) else {
            panic!("{cmd} 不在阻塞档");
        };
        let req = crate::stream::wire::Request {
            id: "t".to_string(),
            cmd: cmd.to_string(),
            args,
            within_ms: None,
            view: Value::Null,
            tz: Default::default(),
            until: None,
        };
        let got = run(req).map(|_| ()).map_err(|f| f.code);
        assert_eq!(got, Err("bad_args".to_string()), "{cmd}");
    }
    assert!(!d.join(".cc-monitor").exists(), "多一个键的那几问写了盘");
    let _ = std::fs::remove_dir_all(&d);
}
