//! 枢纽问的那 8 条内层命令（`HUB_INNER`）是两台后端之间的线上形：远端那台可能是更旧或更新的版本，回包字节换形前后不许变。
//!
//! 换形之前用旧代码跑出来，钉成字面量；换形之后逐字节比（经 `serde_json::Value` 那一跳，同帧面）。临时目录换成 `<D>`。
//! 换形之后入参照旧用 JSON 写、经 `typed::take` 解成入参类型（同帧面那一跳）。

use crate::assets::mcp_sync::{Facts, There};
use crate::stream::inbound::LocalFiles;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 判据仍用 JSON 写入参：经 `typed::take` 解成入参类型（同帧面那一跳），回包序列化回 JSON 比。
fn via<R: serde::de::DeserializeOwned, T: serde::Serialize>(
    args: &Value,
    f: impl FnOnce(&R) -> Result<T, crate::wire::spec::Fail>,
) -> Result<Value, crate::wire::spec::Fail> {
    let req = crate::wire::typed::take::<R>(args)?;
    f(&req).map(|t| serde_json::to_value(t).unwrap())
}

struct NoFacts;
impl Facts for NoFacts {
    fn path(&self, _p: &str) -> There {
        There::Absent
    }
    fn command(&self, _name: &str) -> Option<bool> {
        None
    }
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hub-inner-wire-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// 帧面那一跳的写法（键按字典序），临时目录换成 `<D>`。
fn wire(v: &impl serde::Serialize, d: &Path) -> String {
    serde_json::to_string(&serde_json::to_value(v).unwrap())
        .unwrap()
        .replace(&d.display().to_string(), "<D>")
}

/// 钉住的那一份（没有重写开关：换形之后对不上就是线上形变了）。
fn pin(name: &str, got: String, want: &str) {
    assert_eq!(got, want, "{name}");
}

fn put(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn ok_record(
    _: &crate::assets::skill_ledger::RecordReq,
) -> Result<crate::assets::skill_ledger::RecordReply, crate::wire::spec::Fail> {
    Ok(crate::assets::skill_ledger::RecordReply {
        changed: true,
        remaining: 1,
        ..Default::default()
    })
}

/// skill 三条：来源读 · 被写那台判 · 写。
#[test]
fn the_skill_trio_keeps_its_wire_bytes() {
    let d = dir("skill");
    let src = d.join("src");
    let dst = d.join("dst");
    put(
        &src,
        "demo/SKILL.md",
        "---\nname: demo\n---\nrun /opt/tool\n",
    );
    put(&src, "demo/bin/x.sh", "#!/usr/bin/env zz-missing\n");
    put(&dst, "demo/SKILL.md", "old\n");
    let read = via(&json!({ "name": "demo" }), |r| {
        crate::assets::skill_install::answer_read_at(Some(&src), r)
    })
    .unwrap();
    pin("READ", wire(&read, &d), READ);
    let source: Vec<Value> = read["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| json!({ "path": f["path"], "text": f["text"], "exec": f["exec"] }))
        .collect();
    let look = json!({ "name": "demo", "source": source, "project": null });
    let plan = via(&look, |r| {
        crate::assets::skill_install::answer_plan_with(&NoFacts, Some(&dst), r)
    })
    .unwrap();
    pin("PLAN", wire(&plan, &d), PLAN);
    let mut take = look.clone();
    take["take"] = json!(["SKILL.md", "bin/x.sh"]);
    take["overwrite"] = json!(["SKILL.md"]);
    let taken = via(&take, |r| {
        crate::assets::skill_install::answer_plan_with(&NoFacts, Some(&dst), r)
    })
    .unwrap();
    pin("PLAN_TAKEN", wire(&taken, &d), PLAN_TAKEN);
    let mut apply = take.clone();
    apply["target"] = plan["target"].clone();
    let done = via(&apply, |r| {
        crate::assets::skill_flow::answer_install(&LocalFiles, &NoFacts, Some(&dst), &ok_record, r)
    })
    .unwrap();
    pin("APPLY", wire(&done, &d), APPLY);
    let again = via(&apply, |r| {
        crate::assets::skill_flow::answer_install(&LocalFiles, &NoFacts, Some(&dst), &ok_record, r)
    });
    pin(
        "APPLY_AGAIN",
        wire(&again.map_err(|f| f.code), &d),
        APPLY_AGAIN,
    );
}

/// MCP 三条：来源交空位 · 被写那台看 · 写。
#[test]
fn the_mcp_trio_keeps_its_wire_bytes() {
    let d = dir("mcp");
    let (src, dst) = (d.join("src"), d.join("dst"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(
        src.join(".mcp.json"),
        json!({ "mcpServers": { "s": { "command": "/opt/srv", "env": { "K": "secret" } } } })
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        dst.join(".mcp.json"),
        json!({ "mcpServers": { "s": { "command": "old", "env": { "K": "kept" } } } }).to_string(),
    )
    .unwrap();
    let at = |p: &Path| json!({ "level": "project", "dir": p.display().to_string() });
    let source = via(&json!({ "name": "s", "at": at(&src) }), |r| {
        crate::assets::mcp_sync_flow::answer_source(&LocalFiles, None, r)
    })
    .unwrap();
    pin("SOURCE", wire(&source, &d), SOURCE);
    let pre = via(
        &json!({ "name": "s", "at": at(&dst), "def": source["def"] }),
        |r| crate::assets::mcp_sync_flow::answer_preview(&LocalFiles, &NoFacts, r),
    )
    .unwrap();
    pin("PREVIEW", wire(&pre, &d), PREVIEW);
    let apply = json!({ "name": "s", "at": at(&dst), "def": source["def"], "fill": null, "target": pre["target"] });
    let done = via(&apply, |r| {
        crate::assets::mcp_sync_flow::answer_apply(&LocalFiles, &ok_record, r)
    })
    .unwrap();
    pin("MCP_APPLY", wire(&done, &d), MCP_APPLY);
    let none = via(
        &json!({ "name": "s", "at": at(&d.join("none")), "def": source["def"] }),
        |r| crate::assets::mcp_sync_flow::answer_preview(&LocalFiles, &NoFacts, r),
    );
    pin(
        "PREVIEW_ABSENT",
        wire(&none.map_err(|f| f.code), &d),
        PREVIEW_ABSENT,
    );
}

/// cc-bus 两条：看一眼 · 装（空目录 ⇒ 全写；再装一次 ⇒ 一个字节不写）。
#[test]
fn the_cc_bus_pair_keeps_its_wire_bytes() {
    let d = dir("ccbus");
    let skills = d.join("skills");
    std::fs::create_dir_all(&skills).unwrap();
    let digest = crate::assets::cc_bus_install::embedded_digest();
    let st = serde_json::to_value(crate::assets::cc_bus_install::state_at(&skills)).unwrap();
    let files = serde_json::to_string(&st["writes"]).unwrap();
    assert!(st["writes"].as_array().unwrap().len() > 3, "空目录该全写");
    assert_eq!(
        wire(&st, &d).replace(&digest, "<V>"),
        format!(
            r#"{{"dest":"<D>/skills/cc-bus","existing":false,"version":"<V>","writes":{files}}}"#
        )
    );
    let first =
        crate::assets::cc_bus_install::install_at(&LocalFiles, &skills, &ok_record).unwrap();
    assert_eq!(
        wire(&first, &d),
        format!(
            r#"{{"backup":null,"dest":"<D>/skills/cc-bus","recordFailed":null,"written":{files}}}"#
        )
    );
    let again =
        crate::assets::cc_bus_install::install_at(&LocalFiles, &skills, &ok_record).unwrap();
    assert_eq!(
        wire(&again, &d),
        r#"{"backup":null,"dest":"<D>/skills/cc-bus","recordFailed":null,"written":[]}"#
    );
    let st2 = crate::assets::cc_bus_install::state_at(&skills);
    assert_eq!(
        wire(&st2, &d).replace(&digest, "<V>"),
        r#"{"dest":"<D>/skills/cc-bus","existing":true,"version":"<V>","writes":[]}"#
    );
}

const READ: &str = r##"{"dir":"<D>/src/demo","files":[{"bytes":33,"exec":false,"path":"SKILL.md","text":"---\nname: demo\n---\nrun /opt/tool\n","why":null},{"bytes":26,"exec":false,"path":"bin/x.sh","text":"#!/usr/bin/env zz-missing\n","why":null}],"root":"<D>/src","skipped":[]}"##;
const PLAN: &str = r##"{"base":"<D>/dst","dir":"<D>/dst/demo","ledger":null,"prefix":"demo","root":"<D>/dst","rows":[{"blocked":null,"path":"SKILL.md","state":"differs","suspects":[{"kind":"abs-path","there":"absent","value":"/opt/tool"}]},{"blocked":null,"path":"bin/x.sh","state":"new","suspects":[{"kind":"executable","there":null,"value":""},{"kind":"command-missing","there":"unknown","value":"zz-missing"}]}],"target":[{"path":"SKILL.md","text":"old\n"}],"write":null}"##;
const PLAN_TAKEN: &str = r##"{"base":"<D>/dst","dir":"<D>/dst/demo","ledger":{"SKILL.md":{"created":false,"digest":"dbc4aebaf709b4f8"},"bin/x.sh":{"created":true,"digest":"6243239a06fb14f1"}},"prefix":"demo","root":"<D>/dst","rows":[{"blocked":null,"path":"SKILL.md","state":"differs","suspects":[{"kind":"abs-path","there":"absent","value":"/opt/tool"}]},{"blocked":null,"path":"bin/x.sh","state":"new","suspects":[{"kind":"executable","there":null,"value":""},{"kind":"command-missing","there":"unknown","value":"zz-missing"}]}],"target":[{"path":"SKILL.md","text":"old\n"}],"write":["SKILL.md","bin/x.sh"]}"##;
const APPLY: &str = r##"{"chmodFailed":[],"dir":"<D>/dst/demo","recordFailed":null,"written":["SKILL.md","bin/x.sh"]}"##;
const SOURCE: &str = r##"{"def":{"command":"/opt/srv","env":{"K":null}},"path":"<D>/src/.mcp.json","slots":[{"field":"env","key":"K"}],"token":"ebd76e0d40fd9fff"}"##;
const PREVIEW: &str = r##"{"def":{"command":"/opt/srv","env":{"K":null}},"path":"<D>/dst/.mcp.json","slots":[{"field":"env","kept":true,"key":"K"}],"state":"differs","suspects":[{"field":"command","kind":"abs-path","there":"absent","value":"/opt/srv"}],"target":"747331d9f357d2f9"}"##;
const MCP_APPLY: &str = r##"{"path":"<D>/dst/.mcp.json","recordFailed":null,"written":true}"##;
const PREVIEW_ABSENT: &str = r##"{"Err":"refused"}"##;
const APPLY_AGAIN: &str =
    r##"{"Ok":{"chmodFailed":[],"dir":"<D>/dst/demo","recordFailed":null,"written":[]}}"##;

/// 入参多一个键 ⇒ `bad_args`（入参照拒；回包不拒多的键，见 `hub_tests`）。经本进程那一行登记跑（同枢纽本机那一跳）。
#[test]
fn an_inner_request_with_an_extra_key_is_bad_args() {
    use crate::wire::spec::{invoke, Prepared};
    let cases = [
        ("skill-read", json!({ "name": "demo", "bogus": 1 })),
        (
            "skill-install-plan",
            json!({ "name": "demo", "source": [], "bogus": 1 }),
        ),
        (
            "mcp-sync-source",
            json!({ "name": "s", "at": { "level": "user" }, "bogus": 1 }),
        ),
        (
            "mcp-sync-preview",
            json!({ "name": "s", "at": { "level": "user" }, "def": {}, "bogus": 1 }),
        ),
        ("cc-bus-install-state", json!({ "bogus": 1 })),
    ];
    for (cmd, args) in cases {
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == cmd)
            .unwrap();
        let Some(Prepared::Blocking(run)) = invoke(spec.run, &crate::stream::inbound::hands(None))
        else {
            panic!("{cmd} 不在阻塞档");
        };
        let req = crate::wire::Request {
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
}
